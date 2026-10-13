//! Bounded asynchronous object encryption using RFC 9180 HPKE, not an online
//! channel frame. Classical suite (X25519/HKDF-SHA256/ChaCha20Poly1305), NOT a
//! post-quantum or ratcheted messaging claim. The app authenticates key ownership
//! and signed contents. Each replica gets fresh encapsulation and exterior ID.
use anyhow::{ensure, Result};
use hpke::{
    aead::ChaCha20Poly1305, kdf::HkdfSha256, kem::X25519HkdfSha256, Deserializable, Kem,
    Serializable,
};
use rand::RngCore;
use rand09::SeedableRng;
use zeroize::Zeroizing;
type K = X25519HkdfSha256;
const HEADER: &[u8; 8] = b"NVHPKE01";
const INFO: &[u8] = b"novonet.mailbox.hpke.X25519.HKDF-SHA256.ChaCha20Poly1305.v1\0";
pub const MAX_PLAINTEXT: usize = 48 * 1024;

/// Independent mailbox-only IKM, supplied by platform protected storage. Do not
/// supply a discovery capability, signing seed or wallet spend private key.
pub struct MailboxSecretV1(Zeroizing<[u8; 32]>);
impl MailboxSecretV1 {
    pub fn generate() -> Result<Self> {
        let mut seed = Zeroizing::new([0; 32]);
        rand::rngs::OsRng.try_fill_bytes(seed.as_mut())?;
        Self::restore(seed)
    }
    pub fn restore(seed: Zeroizing<[u8; 32]>) -> Result<Self> {
        ensure!(*seed != [0; 32], "empty mailbox seed");
        Ok(Self(seed))
    }
    /// Protected persistence only; no Serialize/Debug implementation.
    pub fn protected_seed(&self) -> Zeroizing<[u8; 32]> {
        self.0.clone()
    }
    pub fn public_key(&self) -> [u8; 32] {
        K::derive_keypair(self.0.as_ref()).1.to_bytes().into()
    }
    pub fn open(&self, sealed: &[u8], aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        ensure!(
            sealed.len() > 56
                && sealed.len() <= MAX_PLAINTEXT + 56
                && sealed.starts_with(HEADER)
                && aad.len() <= 1024,
            "mailbox sealed bounds/version"
        );
        let (secret, _) = K::derive_keypair(self.0.as_ref());
        let enc = <K as Kem>::EncappedKey::from_bytes(&sealed[8..40])?;
        Ok(Zeroizing::new(hpke::single_shot_open::<
            ChaCha20Poly1305,
            HkdfSha256,
            K,
        >(
            &hpke::OpModeR::Base,
            &secret,
            &enc,
            INFO,
            &sealed[40..],
            aad,
        )?))
    }
}
pub fn seal_mailbox_v1(public: [u8; 32], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        !plaintext.is_empty() && plaintext.len() <= MAX_PLAINTEXT && aad.len() <= 1024,
        "mailbox plain budget"
    );
    let key = <K as Kem>::PublicKey::from_bytes(&public)?;
    let mut seed = Zeroizing::new([0; 32]);
    rand::rngs::OsRng.try_fill_bytes(seed.as_mut())?;
    let mut rng = rand09::rngs::StdRng::from_seed(*seed);
    let (enc, ciphertext) = hpke::single_shot_seal::<ChaCha20Poly1305, HkdfSha256, K, _>(
        &hpke::OpModeS::Base,
        &key,
        INFO,
        plaintext,
        aad,
        &mut rng,
    )?;
    let mut bytes = HEADER.to_vec();
    bytes.extend_from_slice(&enc.to_bytes());
    bytes.extend_from_slice(&ciphertext);
    Ok(bytes)
}
