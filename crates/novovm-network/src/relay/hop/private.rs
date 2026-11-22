//! Sealed setup for a two-forwarder NovoRUDP circuit. The carrier supplies the
//! authenticated immediate peer. Each forwarder learns only its successor and
//! forwards a sealed setup layer and opaque endpoint records. The terminal has
//! the independent record keys. Length, timing and adjacent links are visible.
use super::{MAX_INNER, PRODUCT_OVERLAY_CLOCK_SKEW_MS_V1};
use crate::{
    mailbox_seal::{seal_bounded, MailboxSecretV1},
    product_overlay::{
        TransportIdentity, TransportVerifier, MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
    },
};
use anyhow::{ensure, Context, Result};
use chacha20poly1305::{aead::AeadInPlace, ChaCha20Poly1305, KeyInit, Nonce};
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use sha2::Sha256;
use std::{collections::BTreeMap, sync::Mutex};
use zeroize::Zeroizing;

mod link;
pub use link::{PrivateCircuit, PrivateHopLink, PRIVATE_HOP_ENTRY};
pub(crate) use link::{PrivateReadHalf, PrivateWriteHalf};
#[cfg(test)]
mod tests;

const LAYER: &[u8; 8] = b"NVHSEAL1";
const LAYER_INFO: &[u8] = b"novonet.hop.private.layer.v1\0";
const KEY_CONTEXT: &[u8] = b"NOVONET/private-hop-key/v1";
const KEY_WIRE: &[u8; 8] = b"NVPHKEY1";
const KEY_FIELDS: usize = 32 + 1 + 32 + 8 + 8;
const RECORD_INFO: &[u8] = b"novonet.hop.private.records.v1\0";
const READY: &[u8] = b"NOVONET/private-hop-terminal-ready/v1";
const SETUP_MS: u64 = 30_000;
const MIN_SETUP_MS: u64 = 20_000;
const KEY_MS: u64 = 300_000;
const REPLAY_CAPACITY: usize = 256;
const HEADER_LEN: usize = 8 + 16 + MLDSA65_PUBLIC_KEY_BYTES;
const MAX_LAYER: usize = MAX_INNER - 56;
const TAG_LEN: usize = 16;
pub const MAX_PRIVATE_RECORD: usize = MAX_INNER - TAG_LEN;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PrivateHopRole {
    Forwarder = 1,
    Terminal = 2,
}

/// Published by the hop owner alongside its ordinary signed endpoint. The
/// locally configured identity pin is independent of this declaration.
#[derive(Clone)]
pub struct PrivateHopDeclaration {
    pub scope: [u8; 32],
    pub role: PrivateHopRole,
    pub encryption_key: [u8; 32],
    pub issued_ms: u64,
    pub expires_ms: u64,
    pub signature: Vec<u8>,
}

impl PrivateHopDeclaration {
    fn signed_bytes(&self) -> Vec<u8> {
        [
            self.scope.as_slice(),
            &[self.role as u8],
            &self.encryption_key,
            &self.issued_ms.to_be_bytes(),
            &self.expires_ms.to_be_bytes(),
        ]
        .concat()
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        ensure!(
            self.signature.len() == MLDSA65_SIGNATURE_BYTES,
            "private hop signature width"
        );
        Ok([KEY_WIRE.as_slice(), &self.signed_bytes(), &self.signature].concat())
    }

    /// Relay key announcements are not a trust anchor. Decode only against an
    /// independently pinned signing identity and service scope.
    pub fn verify_wire(
        bytes: &[u8],
        pinned_identity: &[u8],
        scope: [u8; 32],
        now: u64,
        verifier: &dyn TransportVerifier,
    ) -> Result<VerifiedPrivateHop> {
        ensure!(
            bytes.len() == KEY_WIRE.len() + KEY_FIELDS + MLDSA65_SIGNATURE_BYTES
                && bytes.starts_with(KEY_WIRE),
            "private hop announcement shape"
        );
        let bytes = &bytes[KEY_WIRE.len()..];
        let role = match bytes[32] {
            1 => PrivateHopRole::Forwarder,
            2 => PrivateHopRole::Terminal,
            _ => anyhow::bail!("private hop announcement role"),
        };
        Self {
            scope: bytes[..32].try_into()?,
            role,
            encryption_key: bytes[33..65].try_into()?,
            issued_ms: u64::from_be_bytes(bytes[65..73].try_into()?),
            expires_ms: u64::from_be_bytes(bytes[73..81].try_into()?),
            signature: bytes[KEY_FIELDS..].to_vec(),
        }
        .verify(pinned_identity, scope, now, verifier)
    }

    pub fn verify(
        &self,
        pinned_identity: &[u8],
        scope: [u8; 32],
        now: u64,
        verifier: &dyn TransportVerifier,
    ) -> Result<VerifiedPrivateHop> {
        ensure!(
            valid_identity(pinned_identity)
                && self.signature.len() == MLDSA65_SIGNATURE_BYTES
                && self.scope == scope
                && scope != [0; 32]
                && self.encryption_key != [0; 32]
                && self.issued_ms <= now.saturating_add(PRODUCT_OVERLAY_CLOCK_SKEW_MS_V1)
                && self.expires_ms > now
                && (1..=KEY_MS).contains(&self.expires_ms.saturating_sub(self.issued_ms)),
            "private hop declaration scope, key or lease rejected"
        );
        verifier.verify(
            pinned_identity,
            &self.signed_bytes(),
            KEY_CONTEXT,
            &self.signature,
        )?;
        Ok(VerifiedPrivateHop {
            identity: pinned_identity.to_vec(),
            declaration: self.clone(),
        })
    }
}

pub struct VerifiedPrivateHop {
    identity: Vec<u8>,
    declaration: PrivateHopDeclaration,
}

impl VerifiedPrivateHop {
    pub fn role(&self) -> PrivateHopRole {
        self.declaration.role
    }
}

fn valid_identity(key: &[u8]) -> bool {
    key.len() == MLDSA65_PUBLIC_KEY_BYTES && key.iter().any(|byte| *byte != 0)
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    OsRng.try_fill_bytes(&mut bytes)?;
    Ok(bytes)
}

#[derive(Default)]
struct ReplayWindow {
    last_ms: u64,
    used: BTreeMap<[u8; 16], u64>,
}

/// One ephemeral route key and replay window per running service key epoch.
/// A service restart generates and publishes a new key, closing the old window.
pub struct PrivateHopKey {
    secret: MailboxSecretV1,
    identity: Vec<u8>,
    declaration: PrivateHopDeclaration,
    replay: Mutex<ReplayWindow>,
}

pub enum OpenedPrivateHop {
    Forward { next: Vec<u8>, sealed: Vec<u8> },
    Terminal(PrivateEndpoint),
}

impl PrivateHopKey {
    pub fn issue(
        signer: &TransportIdentity,
        scope: [u8; 32],
        role: PrivateHopRole,
        now: u64,
    ) -> Result<Self> {
        ensure!(scope != [0; 32], "empty private hop scope");
        // Fresh material and a separate HPKE domain from mailbox contents.
        let secret = MailboxSecretV1::generate()?;
        let mut declaration = PrivateHopDeclaration {
            scope,
            role,
            encryption_key: secret.public_key(),
            issued_ms: now,
            expires_ms: now.checked_add(KEY_MS).context("private key expiry")?,
            signature: Vec::new(),
        };
        declaration.signature = signer.sign(&declaration.signed_bytes(), KEY_CONTEXT)?;
        Ok(Self {
            secret,
            identity: signer.public_key().to_vec(),
            declaration,
            replay: Mutex::new(ReplayWindow::default()),
        })
    }

    pub fn declaration(&self) -> &PrivateHopDeclaration {
        &self.declaration
    }

    fn open(
        &self,
        authenticated_previous: &[u8],
        sealed: &[u8],
        now: u64,
    ) -> Result<OpenedPrivateHop> {
        ensure!(now < self.declaration.expires_ms, "private hop key expired");
        let layer = self.secret.open_bounded(
            sealed,
            &self.declaration.scope,
            LAYER,
            LAYER_INFO,
            MAX_LAYER,
        )?;
        ensure!(layer.len() > HEADER_LEN, "private hop layer truncated");
        let expires = u64::from_be_bytes(layer[..8].try_into()?);
        let nonce: [u8; 16] = layer[8..24].try_into()?;
        ensure!(
            expires > now
                && expires <= self.declaration.expires_ms
                && expires <= now.saturating_add(SETUP_MS + PRODUCT_OVERLAY_CLOCK_SKEW_MS_V1)
                && nonce != [0; 16]
                && valid_identity(authenticated_previous)
                && layer[24..HEADER_LEN] == *authenticated_previous
                && self.identity != authenticated_previous,
            "private hop predecessor or lease rejected"
        );
        let body = &layer[HEADER_LEN..];
        let opened = match self.declaration.role {
            PrivateHopRole::Forwarder => {
                ensure!(
                    body.len() > MLDSA65_PUBLIC_KEY_BYTES,
                    "private hop successor missing"
                );
                let (next, sealed) = body.split_at(MLDSA65_PUBLIC_KEY_BYTES);
                ensure!(
                    valid_identity(next)
                        && next != authenticated_previous
                        && self.identity != next
                        && sealed.len() > 56
                        && sealed.starts_with(LAYER),
                    "private hop successor or next layer rejected"
                );
                OpenedPrivateHop::Forward {
                    next: next.to_vec(),
                    sealed: sealed.to_vec(),
                }
            }
            PrivateHopRole::Terminal => {
                let seed: &[u8; 32] = body.try_into().context("private terminal setup length")?;
                OpenedPrivateHop::Terminal(PrivateEndpoint::new(
                    seed,
                    self.declaration.scope,
                    false,
                )?)
            }
        };
        let mut replay = self
            .replay
            .lock()
            .map_err(|_| anyhow::anyhow!("private hop replay owner unavailable"))?;
        ensure!(now >= replay.last_ms, "private hop clock rollback");
        replay.last_ms = now;
        replay.used.retain(|_, until| *until > now);
        ensure!(
            !replay.used.contains_key(&nonce),
            "private hop layer replay"
        );
        ensure!(
            replay.used.len() < REPLAY_CAPACITY,
            "private hop replay budget exhausted"
        );
        replay.used.insert(nonce, expires);
        Ok(opened)
    }
}

/// A client-owned selection of two forwarders and a terminal. Separate keys
/// and an encrypted setup do not establish operator independence or validate
/// the mobile discovery, mailbox and receipt path as a whole.
pub struct PrivateRoute {
    first_identity: Vec<u8>,
    first_layer: Vec<u8>,
    endpoint: PrivateEndpoint,
}

impl PrivateRoute {
    pub fn first_identity(&self) -> &[u8] {
        &self.first_identity
    }

    pub fn new(origin: &[u8], hops: [&VerifiedPrivateHop; 3], now: u64) -> Result<Self> {
        let scope = hops[0].declaration.scope;
        ensure!(valid_identity(origin), "private route origin rejected");
        let mut setup_until = [0; 3];
        for (index, hop) in hops.iter().enumerate() {
            let expected = if index == 2 {
                PrivateHopRole::Terminal
            } else {
                PrivateHopRole::Forwarder
            };
            ensure!(
                hop.declaration.scope == scope
                    && hop.declaration.role == expected
                    && hop.declaration.expires_ms.saturating_sub(now) >= SETUP_MS
                    && hop.identity != origin
                    && hops[..index]
                        .iter()
                        .all(|other| other.identity != hop.identity
                            && other.declaration.encryption_key != hop.declaration.encryption_key),
                "private route roles, scope or repeated hop rejected"
            );
            // Distinct wire-layer expiry values avoid adding a shared exact
            // timestamp to the metadata visible to the individual relays.
            let jitter =
                u64::from(u32::from_be_bytes(random::<4>()?)) % (SETUP_MS - MIN_SETUP_MS - 3);
            setup_until[index] = now
                .checked_add(MIN_SETUP_MS + jitter + index as u64)
                .context("private setup clock overflow")?;
            while setup_until[..index].contains(&setup_until[index]) {
                setup_until[index] += 1;
            }
        }
        let seed = Zeroizing::new(random::<32>()?);
        let endpoint = PrivateEndpoint::new(&seed, scope, true)?;
        let mut sealed = Vec::new();
        for (index, hop) in hops.iter().enumerate().rev() {
            let previous = if index == 0 {
                origin
            } else {
                &hops[index - 1].identity
            };
            let mut layer = Zeroizing::new(Vec::with_capacity(
                HEADER_LEN + MLDSA65_PUBLIC_KEY_BYTES + sealed.len(),
            ));
            layer.extend_from_slice(&setup_until[index].to_be_bytes());
            layer.extend_from_slice(&random::<16>()?);
            layer.extend_from_slice(previous);
            if let Some(next) = hops.get(index + 1) {
                layer.extend_from_slice(&next.identity);
                layer.extend_from_slice(&sealed);
            } else {
                layer.extend_from_slice(seed.as_ref());
            }
            sealed = seal_bounded(
                hop.declaration.encryption_key,
                &layer,
                &scope,
                LAYER,
                LAYER_INFO,
                MAX_LAYER,
            )?;
        }
        Ok(Self {
            first_identity: hops[0].identity.clone(),
            first_layer: sealed,
            endpoint,
        })
    }
}

/// End-to-end record protection independent of the adjacent transport keys.
/// The origin cannot emit business bytes before authenticating the terminal.
pub struct PrivateEndpoint {
    send: ChaCha20Poly1305,
    receive: ChaCha20Poly1305,
    sent: u64,
    received: u64,
    initiator: bool,
    ready: bool,
    binding_seed: Zeroizing<[u8; 96]>,
}

impl PrivateEndpoint {
    fn new(seed: &[u8; 32], scope: [u8; 32], initiator: bool) -> Result<Self> {
        let mut keys = Zeroizing::new([0; 64]);
        let keys_owner = Hkdf::<Sha256>::new(Some(&scope), seed);
        keys_owner
            .expand(RECORD_INFO, keys.as_mut())
            .map_err(|_| anyhow::anyhow!("private record key length"))?;
        let mut binding_seed = Zeroizing::new([0; 96]);
        keys_owner
            .expand(b"novonet.hop.private.binding.v1\0", binding_seed.as_mut())
            .map_err(|_| anyhow::anyhow!("private channel binding key length"))?;
        let (forward, reverse) = keys.split_at(32);
        let (send, receive) = if initiator {
            (forward, reverse)
        } else {
            (reverse, forward)
        };
        Ok(Self {
            send: ChaCha20Poly1305::new_from_slice(send)?,
            receive: ChaCha20Poly1305::new_from_slice(receive)?,
            sent: 0,
            received: 0,
            initiator,
            ready: false,
            binding_seed,
        })
    }

    fn terminal_ready(&mut self) -> Result<Vec<u8>> {
        ensure!(
            !self.initiator && !self.ready,
            "private terminal readiness state"
        );
        let proof = self.seal(READY)?;
        self.ready = true;
        Ok(proof)
    }

    fn confirm_terminal(&mut self, proof: &[u8]) -> Result<()> {
        ensure!(
            self.initiator && !self.ready && self.received == 0,
            "private origin readiness state"
        );
        ensure!(
            self.open(proof)? == READY,
            "private terminal confirmation rejected"
        );
        self.ready = true;
        Ok(())
    }

    fn send(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        ensure!(self.ready, "private endpoint not ready");
        self.seal(bytes)
    }

    fn receive(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        ensure!(self.ready, "private endpoint not ready");
        self.open(bytes)
    }

    fn seal(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        Self::seal_record(&self.send, &mut self.sent, bytes)
    }

    fn open(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        Self::open_record(&self.receive, &mut self.received, bytes)
    }

    fn seal_record(cipher: &ChaCha20Poly1305, sent: &mut u64, bytes: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_PRIVATE_RECORD,
            "private record plaintext length"
        );
        let next = sent
            .checked_add(1)
            .context("private send sequence exhausted")?;
        let mut data = bytes.to_vec();
        cipher
            .encrypt_in_place(&nonce(*sent), RECORD_INFO, &mut data)
            .map_err(|_| anyhow::anyhow!("private record sealing failed"))?;
        *sent = next;
        Ok(data)
    }

    fn open_record(cipher: &ChaCha20Poly1305, received: &mut u64, bytes: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            bytes.len() > TAG_LEN && bytes.len() <= MAX_INNER,
            "private record ciphertext length"
        );
        let next = received
            .checked_add(1)
            .context("private receive sequence exhausted")?;
        let mut data = bytes.to_vec();
        cipher
            .decrypt_in_place(&nonce(*received), RECORD_INFO, &mut data)
            .map_err(|_| anyhow::anyhow!("private record authentication failed"))?;
        *received = next;
        Ok(data)
    }
}

fn nonce(sequence: u64) -> Nonce {
    let mut bytes = [0; 12];
    bytes[4..].copy_from_slice(&sequence.to_be_bytes());
    bytes.into()
}
