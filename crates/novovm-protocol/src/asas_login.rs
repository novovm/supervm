//! Off-chain device possession, distinct from UCA grants and asset signatures.
use crate::uca_application::ApplicationGrantSelectorV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CONTEXT: &[u8] = b"NOVOVM/ASAS/device-login/v1";
pub fn application_id() -> [u8; 32] {
    Sha256::digest(b"AOEM/ASAS/application/v1\0").into()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginChallengeV1 {
    pub version: u8,
    pub selector: ApplicationGrantSelectorV1,
    pub revision: u64,
    pub session: [u8; 16],
    pub nonce: [u8; 32],
    pub expires_at_ms: u64,
}
impl LoginChallengeV1 {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, &'static str> {
        self.selector
            .validate()
            .map_err(|_| "invalid login selector")?;
        if self.version != 1
            || self.selector.application != application_id()
            || self.revision == 0
            || self.nonce == [0; 32]
            || self.expires_at_ms == 0
        {
            return Err("invalid ASAS challenge");
        }
        let mut bytes = CONTEXT.to_vec();
        bytes.push(0);
        bytes.extend(postcard::to_allocvec(self).map_err(|_| "invalid login encoding")?);
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginProofV1 {
    pub challenge: LoginChallengeV1,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}
