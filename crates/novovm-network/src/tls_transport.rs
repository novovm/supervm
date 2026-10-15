//! Authenticated, framed TLS 1.3 over an owned reliable byte stream.
//!
//! This module never opens sockets, resolves names, or asserts anonymity. Its
//! caller authorizes the actual carrier before creating the supplied IO. Both
//! peers prove independent, ephemeral Ed25519 raw transport keys in real TLS;
//! NOVOVM identity and application authority still require the bound handshake.
//! No public API imports a caller's claimed exporter or verified binding.

use crate::transport_binding::{
    VerifiedTransportBinding, BOUND_TRANSPORT_ALPN_V1, BOUND_TRANSPORT_EXPORTER_CONTEXT_V1,
    BOUND_TRANSPORT_EXPORTER_LABEL_V1,
};
use anyhow::{ensure, Context, Result};
use curve25519_dalek::edwards::CompressedEdwardsY;
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use rand::RngCore;
use rustls::{
    client::{
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        AlwaysResolvesClientRawPublicKeys, Resumption,
    },
    pki_types::{CertificateDer, ServerName, SubjectPublicKeyInfoDer, UnixTime},
    server::{
        danger::{ClientCertVerified, ClientCertVerifier},
        AlwaysResolvesServerRawPublicKeys, NoServerSessionStorage,
    },
    sign::{CertifiedKey, Signer, SigningKey as TlsSigningKey},
    CertificateError, ClientConfig, DigitallySignedStruct, DistinguishedName, Error as TlsError,
    ProtocolVersion, ServerConfig, SignatureAlgorithm, SignatureScheme,
};
use std::{
    fmt,
    future::Future,
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    task::{Context as TaskContext, Poll},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf, ReadHalf, WriteHalf},
    sync::{OwnedSemaphorePermit, Semaphore, SemaphorePermit},
};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use zeroize::Zeroizing;

pub const TLS_MAX_FRAME_V1: usize = 16 * 1024;
pub const TLS_MAX_CONNECTIONS_V1: usize = 4;
const CHECK_INTERVAL: Duration = Duration::from_millis(20);
const SERVICE_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
// RFC 8410: exact, canonical Ed25519 SubjectPublicKeyInfo with absent parameters.
const ED25519_SPKI_PREFIX: &[u8] = &[
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];
static SCOPE_LIMIT: OnceLock<Arc<Semaphore>> = OnceLock::new();
#[cfg(test)]
pub(crate) static TLS_TEST_SERIAL: Semaphore = Semaphore::const_new(1);

/// Only actual byte-stream IO failures receive this marker. TLS verification,
/// malformed records, bounds, authorization and cancellation remain terminal
/// unclassified errors. A marker itself does not authorize retry or downgrade.
#[derive(Debug, thiserror::Error)]
#[error("TLS carrier stream IO failed: {0:#}")]
pub struct TlsStreamIoFailureV1(#[source] anyhow::Error);

#[derive(Clone)]
pub struct TlsControlV1 {
    deadline: Option<Instant>,
    authority: Arc<dyn Fn() -> Result<()> + Send + Sync>,
}
impl TlsControlV1 {
    pub fn new(
        deadline: Instant,
        authority: impl Fn() -> Result<()> + Send + Sync + 'static,
    ) -> Result<Self> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(
            !remaining.is_zero() && remaining <= Duration::from_secs(120),
            "TLS carrier requires a lease of at most 120 seconds"
        );
        let value = Self {
            deadline: Some(deadline),
            authority: Arc::new(authority),
        };
        value.check()?;
        Ok(value)
    }
    fn check(&self) -> Result<()> {
        (self.authority)()?;
        ensure!(
            self.deadline.is_none_or(|end| Instant::now() < end),
            "TLS carrier scope expired"
        );
        Ok(())
    }
    async fn during<T>(
        &self,
        state: &ConnectionState,
        future: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        tokio::pin!(future);
        loop {
            state.check(self)?;
            tokio::select! {
                biased;
                result = &mut future => {
                    // Preserve an observed protocol/identity error before any
                    // adjacent timer can replace it. Successful IO rechecks.
                    let value = result?;
                    state.check(self)?;
                    return Ok(value);
                }
                _ = tokio::time::sleep(CHECK_INTERVAL) => {}
            }
        }
    }
}

trait TransportIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> TransportIo for T {}
type BoxIo = Box<dyn TransportIo + 'static>;
type TlsIo = tokio_rustls::TlsStream<ClosableIo>;

// The lock is held for one nonblocking poll only, never across an await. The
// independent abort handle can actually drop owned IO when either half fails.
#[derive(Clone)]
struct ClosableIo(Arc<Mutex<Option<BoxIo>>>);
impl ClosableIo {
    fn new(io: impl TransportIo + 'static) -> Self {
        Self(Arc::new(Mutex::new(Some(Box::new(io)))))
    }
    fn close(&self) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).take();
    }
    fn poll<T>(
        &self,
        work: impl FnOnce(Pin<&mut dyn TransportIo>) -> Poll<io::Result<T>>,
    ) -> Poll<io::Result<T>> {
        let mut lock = match self.0.lock() {
            Ok(lock) => lock,
            Err(poisoned) => {
                poisoned.into_inner().take();
                return Poll::Ready(Err(io::Error::other("TLS IO lock poisoned")));
            }
        };
        match lock.as_mut() {
            Some(io) => work(Pin::new(io.as_mut())),
            None => Poll::Ready(Err(io::ErrorKind::NotConnected.into())),
        }
    }
}
impl AsyncRead for ClosableIo {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll(|io| io.poll_read(cx, buf))
    }
}
impl AsyncWrite for ClosableIo {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.poll(|io| io.poll_write(cx, bytes))
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        self.poll(|io| io.poll_flush(cx))
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        self.poll(|io| io.poll_shutdown(cx))
    }
}
struct ConnectionState {
    failed: AtomicBool,
    io: ClosableIo,
    // Shared with the existing IO supervisor. None is authenticated V6 idle,
    // never an unlimited handshake or request.
    request_deadline: Mutex<Option<Instant>>,
}
impl ConnectionState {
    fn fail(&self) {
        self.failed.store(true, Ordering::Release);
        self.io.close();
    }
    fn check(&self, control: &TlsControlV1) -> Result<()> {
        // Preserve the actual current authority/deadline error even when the
        // idle watchdog already closed the transport before this operation.
        if let Err(error) = control.check() {
            self.fail();
            return Err(error);
        }
        ensure!(
            !self.failed.load(Ordering::Acquire),
            "TLS carrier stream failed"
        );
        let deadline = self.request_deadline.lock().map_err(|_| {
            self.fail();
            anyhow::anyhow!("TLS request state poisoned")
        })?;
        if deadline.is_some_and(|end| Instant::now() >= end) {
            self.fail();
            anyhow::bail!("TLS service request expired");
        }
        Ok(())
    }
    fn active_deadline(&self, control: &TlsControlV1) -> Result<Instant> {
        self.check(control)?;
        self.request_deadline
            .lock()
            .map_err(|_| anyhow::anyhow!("TLS request state poisoned"))?
            .context("TLS service request is idle")
    }
    #[cfg(feature = "iroh-transport")]
    fn transition(&self, control: &TlsControlV1, next: Option<Instant>) -> Result<()> {
        self.check(control)?;
        let mut deadline = self
            .request_deadline
            .lock()
            .map_err(|_| anyhow::anyhow!("TLS request state poisoned"))?;
        // Check while holding the same lock as the watchdog: expiration cannot
        // be erased by starting another request at the deadline boundary.
        if deadline.is_some_and(|end| Instant::now() >= end) {
            self.fail();
            anyhow::bail!("TLS service request expired");
        }
        ensure!(
            !self.failed.load(Ordering::Acquire),
            "TLS carrier stream failed"
        );
        *deadline = next;
        Ok(())
    }
    fn io_result<T>(&self, control: &TlsControlV1, result: io::Result<T>) -> Result<T> {
        self.check(control)?;
        result.map_err(|error| {
            if error.kind() == io::ErrorKind::InvalidData
                || error.get_ref().is_some_and(|e| e.is::<TlsError>())
            {
                error.into()
            } else {
                TlsStreamIoFailureV1(error.into()).into()
            }
        })
    }
}
impl Drop for ConnectionState {
    fn drop(&mut self) {
        self.io.close();
    }
}
struct FrameGuard<'a> {
    state: &'a ConnectionState,
    complete: bool,
}
impl Drop for FrameGuard<'_> {
    fn drop(&mut self) {
        if !self.complete {
            self.state.fail();
        }
    }
}
struct SupervisorGuard(Arc<ConnectionState>);
impl Drop for SupervisorGuard {
    fn drop(&mut self) {
        self.0.fail();
    }
}

struct TransportSigner {
    key: Arc<SigningKey>,
    spki: Vec<u8>,
}
impl fmt::Debug for TransportSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EphemeralEd25519TransportSigner")
    }
}
impl TlsSigningKey for TransportSigner {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        offered
            .contains(&SignatureScheme::ED25519)
            .then(|| Box::new(TransportSignature(self.key.clone())) as Box<dyn Signer>)
    }
    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ED25519
    }
    fn public_key(&self) -> Option<SubjectPublicKeyInfoDer<'_>> {
        Some(SubjectPublicKeyInfoDer::from(self.spki.as_slice()))
    }
}
struct TransportSignature(Arc<SigningKey>);
impl fmt::Debug for TransportSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Ed25519TransportSignature")
    }
}
impl Signer for TransportSignature {
    fn sign(&self, message: &[u8]) -> std::result::Result<Vec<u8>, TlsError> {
        Ok(self.0.sign(message).to_bytes().to_vec())
    }
    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
    }
}
fn bad_key() -> TlsError {
    TlsError::InvalidCertificate(CertificateError::BadEncoding)
}
fn public_key(spki: &[u8]) -> std::result::Result<[u8; 32], TlsError> {
    if spki.len() != ED25519_SPKI_PREFIX.len() + 32 || !spki.starts_with(ED25519_SPKI_PREFIX) {
        return Err(bad_key());
    }
    let key: [u8; 32] = spki[ED25519_SPKI_PREFIX.len()..]
        .try_into()
        .map_err(|_| bad_key())?;
    check_public_key(&key)?;
    Ok(key)
}

fn check_public_key(key: &[u8; 32]) -> std::result::Result<(), TlsError> {
    let point = CompressedEdwardsY(*key).decompress().ok_or_else(bad_key)?;
    if point.compress().to_bytes() != *key || !point.is_torsion_free() || point.is_small_order() {
        return Err(bad_key());
    }
    VerifyingKey::from_bytes(key).map_err(|_| bad_key())?;
    Ok(())
}
#[derive(Debug)]
struct RawKeyVerifier {
    expected: Option<[u8; 32]>,
}
impl RawKeyVerifier {
    fn verify_key(
        &self,
        raw: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
    ) -> std::result::Result<(), TlsError> {
        if !intermediates.is_empty() {
            return Err(bad_key());
        }
        let key = public_key(raw.as_ref())?;
        if self.expected.is_some_and(|expected| expected != key) {
            return Err(TlsError::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ));
        }
        Ok(())
    }
    fn signature(
        &self,
        message: &[u8],
        raw: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        self.verify_key(raw, &[])?;
        if dss.scheme != SignatureScheme::ED25519 {
            return Err(bad_key());
        }
        let signature = Signature::from_slice(dss.signature())
            .map_err(|_| TlsError::InvalidCertificate(CertificateError::BadSignature))?;
        VerifyingKey::from_bytes(&public_key(raw.as_ref())?)
            .map_err(|_| bad_key())?
            .verify_strict(message, &signature)
            .map_err(|_| TlsError::InvalidCertificate(CertificateError::BadSignature))?;
        Ok(HandshakeSignatureValid::assertion())
    }
}
impl ServerCertVerifier for RawKeyVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> std::result::Result<ServerCertVerified, TlsError> {
        self.verify_key(end_entity, intermediates)?;
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        Err(bad_key())
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        raw: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        self.signature(message, raw, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
    fn requires_raw_public_keys(&self) -> bool {
        true
    }
}
impl ClientCertVerifier for RawKeyVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }
    fn client_auth_mandatory(&self) -> bool {
        true
    }
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _: UnixTime,
    ) -> std::result::Result<ClientCertVerified, TlsError> {
        self.verify_key(end_entity, intermediates)?;
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        Err(bad_key())
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        raw: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        self.signature(message, raw, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
    fn requires_raw_public_keys(&self) -> bool {
        true
    }
}

/// An owned transport-key lifetime. Finite V1 scopes retain their absolute
/// deadline; continuous scopes retain live owner checks and bounded requests.
/// Neither bootstraps or authorizes the underlying network.
pub struct TlsScopeV1 {
    control: TlsControlV1,
    identity: Arc<CertifiedKey>,
    endpoint: [u8; 32],
    connections: Semaphore,
    _admission: OwnedSemaphorePermit,
}
impl TlsScopeV1 {
    /// Owner-controlled transport lifetime for the existing continuous service
    /// protocol. Setup and each request remain bounded. This does not grant any
    /// mailbox capability or authenticate an application identity.
    #[cfg(feature = "iroh-transport")]
    pub fn new_continuous_v4(
        authority: impl Fn() -> Result<()> + Send + Sync + 'static,
    ) -> Result<Self> {
        Self::new(TlsControlV1 {
            deadline: None,
            authority: Arc::new(authority),
        })
    }
    pub fn new(control: TlsControlV1) -> Result<Self> {
        let mut seed = Zeroizing::new([0; 32]);
        rand::rngs::OsRng
            .try_fill_bytes(seed.as_mut())
            .context("TLS key randomness unavailable")?;
        Self::from_seed(control, seed)
    }

    /// A platform-owned service transport key, separate from wallet spending
    /// and node signing keys. The platform supplies protected persistence.
    #[cfg(feature = "iroh-transport")]
    pub fn new_owned_continuous_v1(
        seed: Zeroizing<[u8; 32]>,
        authority: impl Fn() -> Result<()> + Send + Sync + 'static,
    ) -> Result<Self> {
        Self::from_seed(
            TlsControlV1 {
                deadline: None,
                authority: Arc::new(authority),
            },
            seed,
        )
    }

    fn from_seed(control: TlsControlV1, seed: Zeroizing<[u8; 32]>) -> Result<Self> {
        control.check()?;
        let admission = SCOPE_LIMIT
            .get_or_init(|| Arc::new(Semaphore::new(4)))
            .clone()
            .try_acquire_owned()
            .context("TLS scope capacity exhausted")?;
        let key = Arc::new(SigningKey::from_bytes(&seed));
        let endpoint = key.verifying_key().to_bytes();
        let mut spki = ED25519_SPKI_PREFIX.to_vec();
        spki.extend_from_slice(&endpoint);
        let identity = Arc::new(CertifiedKey::new(
            vec![CertificateDer::from(spki.clone())],
            Arc::new(TransportSigner { key, spki }),
        ));
        control.check()?;
        Ok(Self {
            control,
            identity,
            endpoint,
            connections: Semaphore::new(TLS_MAX_CONNECTIONS_V1),
            _admission: admission,
        })
    }
    /// Local owner resource budget, fixed before any stream borrows the scope.
    /// Remote peer admission and byte/request/storage limits remain independent.
    pub fn with_connection_limit(mut self, limit: usize) -> Result<Self> {
        ensure!((1..=64).contains(&limit), "TLS connection limit bounds");
        self.connections = Semaphore::new(limit);
        Ok(self)
    }
    pub fn endpoint_key(&self) -> [u8; 32] {
        self.endpoint
    }
    fn client_config(&self, expected: [u8; 32]) -> Result<Arc<ClientConfig>> {
        let mut config = ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(RawKeyVerifier {
            expected: Some(expected),
        }))
        .with_client_cert_resolver(Arc::new(AlwaysResolvesClientRawPublicKeys::new(
            self.identity.clone(),
        )));
        config.alpn_protocols = vec![BOUND_TRANSPORT_ALPN_V1.to_vec()];
        config.resumption = Resumption::disabled();
        config.enable_early_data = false;
        config.enable_sni = false;
        Ok(Arc::new(config))
    }
    fn server_config(&self, expected: Option<[u8; 32]>) -> Result<Arc<ServerConfig>> {
        let mut config = ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(Arc::new(RawKeyVerifier { expected }))
        .with_cert_resolver(Arc::new(AlwaysResolvesServerRawPublicKeys::new(
            self.identity.clone(),
        )));
        config.alpn_protocols = vec![BOUND_TRANSPORT_ALPN_V1.to_vec()];
        config.session_storage = Arc::new(NoServerSessionStorage {});
        config.send_tls13_tickets = 0;
        config.max_early_data_size = 0;
        Ok(Arc::new(config))
    }
    pub async fn connect<I: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        &self,
        io: I,
        expected_endpoint: [u8; 32],
    ) -> Result<TlsStreamV1<'_>> {
        self.establish(io, Some(expected_endpoint), true).await
    }
    /// `None` permits any VALID TLS transport key, not an application identity.
    /// The NOVOVM bound handshake must still enforce its independently pinned
    /// identity/allowlist before allowing any RPC or business operation.
    pub async fn accept<I: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        &self,
        io: I,
        expected_endpoint: Option<[u8; 32]>,
    ) -> Result<TlsStreamV1<'_>> {
        self.establish(io, expected_endpoint, false).await
    }
    async fn establish<I: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        &self,
        io: I,
        expected: Option<[u8; 32]>,
        initiator: bool,
    ) -> Result<TlsStreamV1<'_>> {
        let runtime = tokio::runtime::Handle::try_current()
            .context("TLS connection requires a Tokio runtime")?;
        self.control.check()?;
        if let Some(key) = expected {
            check_public_key(&key)?;
            ensure!(key != self.endpoint, "invalid TLS peer key");
        }
        let admission = self
            .connections
            .try_acquire()
            .context("TLS connection capacity exhausted")?;
        let io = ClosableIo::new(io);
        let state = Arc::new(ConnectionState {
            failed: AtomicBool::new(false),
            io: io.clone(),
            request_deadline: Mutex::new(Some(
                self.control
                    .deadline
                    .unwrap_or_else(|| Instant::now() + SERVICE_REQUEST_TIMEOUT),
            )),
        });
        let mut guard = FrameGuard {
            state: &state,
            complete: false,
        };
        let mut tls = self
            .control
            .during(&state, async {
                if initiator {
                    let connector = TlsConnector::from(
                        self.client_config(expected.context("TLS peer pin missing")?)?,
                    );
                    let handshake = connector
                        .connect(
                            ServerName::try_from("bound-carrier.invalid")?.to_owned(),
                            io,
                        )
                        .await;
                    let stream = state.io_result(&self.control, handshake)?;
                    Ok(tokio_rustls::TlsStream::Client(stream))
                } else {
                    let acceptor = TlsAcceptor::from(self.server_config(expected)?);
                    let stream = state.io_result(&self.control, acceptor.accept(io).await)?;
                    Ok(tokio_rustls::TlsStream::Server(stream))
                }
            })
            .await?;
        // A final TLS handshake flight may still be buffered. Nothing can be
        // advertised as written/authenticated until it leaves this adapter.
        self.control
            .during(&state, async {
                state.io_result(&self.control, tls.flush().await)
            })
            .await?;
        let (_, session) = tls.get_ref();
        ensure!(
            !session.is_handshaking()
                && session.protocol_version() == Some(ProtocolVersion::TLSv1_3)
                && session.alpn_protocol() == Some(BOUND_TRANSPORT_ALPN_V1),
            "TLS carrier negotiation differs"
        );
        let certs = session
            .peer_certificates()
            .context("TLS peer transport key missing")?;
        ensure!(certs.len() == 1, "TLS peer transport key count differs");
        let remote = public_key(certs[0].as_ref())?;
        ensure!(
            remote != self.endpoint && expected.is_none_or(|key| key == remote),
            "TLS peer transport key differs"
        );
        let mut exporter = Zeroizing::new([0u8; 32]);
        match &tls {
            tokio_rustls::TlsStream::Client(stream) => {
                stream.get_ref().1.export_keying_material(
                    exporter.as_mut(),
                    BOUND_TRANSPORT_EXPORTER_LABEL_V1,
                    Some(BOUND_TRANSPORT_EXPORTER_CONTEXT_V1),
                )?;
            }
            tokio_rustls::TlsStream::Server(stream) => {
                stream.get_ref().1.export_keying_material(
                    exporter.as_mut(),
                    BOUND_TRANSPORT_EXPORTER_LABEL_V1,
                    Some(BOUND_TRANSPORT_EXPORTER_CONTEXT_V1),
                )?;
            }
        }
        // Validate the evidence before publishing the stream; exporter remains
        // private and is erased with the stream. binding() derives it locally.
        VerifiedTransportBinding::from_authenticated_transport(
            self.endpoint,
            remote,
            *exporter,
            BOUND_TRANSPORT_ALPN_V1,
            initiator,
        )?;
        state.check(&self.control)?;
        guard.complete = true;
        drop(guard);
        // A caller may retain an idle stream without polling any IO future.
        // Keep its declared absolute lifetime real: one bounded supervisor per
        // admitted stream closes actual owned IO on expiry/revocation. It is
        // started only after the TLS identity/exporter is verified, and never
        // prolongs a stream's original lease or opens another connection.
        let supervisor_guard = SupervisorGuard(state.clone());
        let watched_control = self.control.clone();
        let supervisor = runtime.spawn(async move {
            // A panicking authority or an aborted supervisor also closes IO;
            // this guard was made before spawning, so cancellation before the
            // monitor's first poll also closes its actual underlying transport.
            let guard = supervisor_guard;
            loop {
                tokio::time::sleep(CHECK_INTERVAL).await;
                if guard.0.failed.load(Ordering::Acquire) {
                    return;
                }
                if guard.0.check(&watched_control).is_err() {
                    guard.0.fail();
                    return;
                }
            }
        });
        Ok(TlsStreamV1 {
            scope: self,
            tls,
            state,
            remote,
            exporter,
            initiator,
            #[cfg(feature = "iroh-transport")]
            continuous_service: false,
            supervisor,
            _admission: admission,
        })
    }
}

pub struct TlsStreamV1<'scope> {
    scope: &'scope TlsScopeV1,
    tls: TlsIo,
    state: Arc<ConnectionState>,
    remote: [u8; 32],
    exporter: Zeroizing<[u8; 32]>,
    initiator: bool,
    #[cfg(feature = "iroh-transport")]
    continuous_service: bool,
    supervisor: tokio::task::JoinHandle<()>,
    _admission: SemaphorePermit<'scope>,
}
pub struct TlsWriteHalfV1<'a> {
    control: &'a TlsControlV1,
    state: &'a ConnectionState,
    io: WriteHalf<&'a mut TlsIo>,
}
pub struct TlsReadHalfV1<'a> {
    control: &'a TlsControlV1,
    state: &'a ConnectionState,
    io: ReadHalf<&'a mut TlsIo>,
}

async fn write_frame<W: AsyncWrite + Unpin>(
    control: &TlsControlV1,
    state: &ConnectionState,
    io: &mut W,
    bytes: &[u8],
) -> Result<()> {
    state.active_deadline(control)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= TLS_MAX_FRAME_V1,
        "TLS carrier frame limit"
    );
    let mut guard = FrameGuard {
        state,
        complete: false,
    };
    control
        .during(state, async {
            state.io_result(
                control,
                io.write_all(&(bytes.len() as u32).to_be_bytes()).await,
            )?;
            state.io_result(control, io.write_all(bytes).await)?;
            state.io_result(control, io.flush().await)?;
            Ok(())
        })
        .await?;
    guard.complete = true;
    Ok(())
}
async fn read_frame<R: AsyncRead + Unpin>(
    control: &TlsControlV1,
    state: &ConnectionState,
    io: &mut R,
) -> Result<Vec<u8>> {
    state.active_deadline(control)?;
    let mut guard = FrameGuard {
        state,
        complete: false,
    };
    let bytes = control
        .during(state, async {
            let mut length = [0; 4];
            state.io_result(control, io.read_exact(&mut length).await)?;
            let length = u32::from_be_bytes(length) as usize;
            ensure!(
                length > 0 && length <= TLS_MAX_FRAME_V1,
                "TLS carrier frame limit"
            );
            let mut bytes = vec![0; length];
            state.io_result(control, io.read_exact(&mut bytes).await)?;
            Ok(bytes)
        })
        .await?;
    guard.complete = true;
    Ok(bytes)
}
impl TlsWriteHalfV1<'_> {
    pub async fn write_frame(&mut self, bytes: &[u8]) -> Result<()> {
        write_frame(self.control, self.state, &mut self.io, bytes).await
    }
}
impl TlsReadHalfV1<'_> {
    pub async fn read_frame(&mut self) -> Result<Vec<u8>> {
        read_frame(self.control, self.state, &mut self.io).await
    }
}
impl TlsStreamV1<'_> {
    // Only the existing mailbox negotiation invokes these transitions after
    // the bound identity handshake. Finite V1 scopes can never be upgraded.
    #[cfg(feature = "iroh-transport")]
    pub(crate) fn admit_continuous_service_v4(&mut self) -> Result<()> {
        self.state.check(&self.scope.control)?;
        ensure!(
            self.scope.control.deadline.is_none() && !self.continuous_service,
            "TLS carrier has no fresh continuous service scope"
        );
        self.state.transition(
            &self.scope.control,
            Some(Instant::now() + SERVICE_REQUEST_TIMEOUT),
        )?;
        self.continuous_service = true;
        Ok(())
    }
    #[cfg(feature = "iroh-transport")]
    pub(crate) fn begin_service_request_v4(&mut self) -> Result<()> {
        ensure!(
            self.continuous_service,
            "TLS continuous service not negotiated"
        );
        self.state.transition(
            &self.scope.control,
            Some(Instant::now() + SERVICE_REQUEST_TIMEOUT),
        )
    }
    #[cfg(feature = "iroh-transport")]
    pub(crate) fn complete_service_request_v6(&mut self) -> Result<()> {
        ensure!(
            self.continuous_service,
            "TLS continuous service not negotiated"
        );
        self.state.active_deadline(&self.scope.control)?;
        self.state.transition(&self.scope.control, None)
    }
    #[cfg(feature = "iroh-transport")]
    pub(crate) fn service_request_deadline_v6(&mut self) -> Result<Instant> {
        ensure!(
            self.continuous_service,
            "TLS continuous service not negotiated"
        );
        self.state.active_deadline(&self.scope.control)
    }
    #[cfg(feature = "iroh-transport")]
    pub(crate) async fn read_idle_service_frame_v6(&mut self) -> Result<Vec<u8>> {
        let control = &self.scope.control;
        let state = &self.state;
        state.check(control)?;
        ensure!(
            self.continuous_service
                && state
                    .request_deadline
                    .lock()
                    .map_err(|_| anyhow::anyhow!("TLS request state poisoned"))?
                    .is_none(),
            "TLS service is not idle"
        );
        let mut guard = FrameGuard {
            state,
            complete: false,
        };
        let mut length = [0; 4];
        control
            .during(state, async {
                state.io_result(control, self.tls.read_exact(&mut length[..1]).await)?;
                Ok(())
            })
            .await?;
        state.transition(control, Some(Instant::now() + SERVICE_REQUEST_TIMEOUT))?;
        let bytes = control
            .during(state, async {
                state.io_result(control, self.tls.read_exact(&mut length[1..]).await)?;
                let length = u32::from_be_bytes(length) as usize;
                ensure!(
                    length > 0 && length <= TLS_MAX_FRAME_V1,
                    "TLS carrier frame limit"
                );
                let mut bytes = vec![0; length];
                state.io_result(control, self.tls.read_exact(&mut bytes).await)?;
                Ok(bytes)
            })
            .await?;
        guard.complete = true;
        Ok(bytes)
    }
    pub fn binding(&self) -> Result<VerifiedTransportBinding> {
        self.state.check(&self.scope.control)?;
        Ok(VerifiedTransportBinding::from_authenticated_transport(
            self.scope.endpoint,
            self.remote,
            *self.exporter,
            BOUND_TRANSPORT_ALPN_V1,
            self.initiator,
        )?)
    }
    pub fn remote_endpoint_key(&self) -> Result<[u8; 32]> {
        self.state.check(&self.scope.control)?;
        Ok(self.remote)
    }
    pub fn selected_path(&self) -> Result<&'static str> {
        self.state.check(&self.scope.control)?;
        Ok("tls-unclassified")
    }
    pub fn split_io(&mut self) -> Result<(TlsWriteHalfV1<'_>, TlsReadHalfV1<'_>)> {
        self.state.active_deadline(&self.scope.control)?;
        let (read, write) = tokio::io::split(&mut self.tls);
        Ok((
            TlsWriteHalfV1 {
                control: &self.scope.control,
                state: &self.state,
                io: write,
            },
            TlsReadHalfV1 {
                control: &self.scope.control,
                state: &self.state,
                io: read,
            },
        ))
    }
    pub async fn write_frame(&mut self, bytes: &[u8]) -> Result<()> {
        write_frame(&self.scope.control, &self.state, &mut self.tls, bytes).await
    }
    pub async fn read_frame(&mut self) -> Result<Vec<u8>> {
        read_frame(&self.scope.control, &self.state, &mut self.tls).await
    }
    /// Application receipts must already be verified. Flush close_notify,
    /// require the peer's authenticated close with no trailing application
    /// byte, then close underlying IO. Bare EOF is a truncation failure.
    pub async fn finish(&mut self) -> Result<()> {
        let control = &self.scope.control;
        let state = &self.state;
        state.active_deadline(control)?;
        let mut guard = FrameGuard {
            state,
            complete: false,
        };
        self.tls.get_mut().1.send_close_notify();
        control
            .during(state, async {
                state.io_result(control, self.tls.flush().await)?;
                let mut trailing = [0; 1];
                ensure!(
                    state.io_result(control, self.tls.read(&mut trailing).await)? == 0,
                    "unexpected trailing TLS stream data"
                );
                state.io_result(control, self.tls.shutdown().await)?;
                Ok(())
            })
            .await?;
        guard.complete = true;
        // Finished streams are terminal too; no second handshake or reuse.
        state.fail();
        Ok(())
    }
}
impl Drop for TlsStreamV1<'_> {
    fn drop(&mut self) {
        self.supervisor.abort();
        self.state.fail();
    }
}

#[cfg(test)]
#[path = "tls_transport_tests.rs"]
mod tests;
