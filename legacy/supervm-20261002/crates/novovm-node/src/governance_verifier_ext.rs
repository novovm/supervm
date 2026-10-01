use anyhow::{bail, Context, Result};
use aoem_bindings::{default_host_dll_path, AoemMldsaVerifyItemRef};
use novovm_consensus::{
    BFTEngine, BFTError, GovernanceVote, GovernanceVoteVerificationInput,
    GovernanceVoteVerificationReport, GovernanceVoteVerifier, GovernanceVoteVerifierScheme,
};
use std::collections::HashMap;
use std::sync::Arc;

const GOVERNANCE_MLDSA87_ENVELOPE_MAGIC_V1: &[u8] = b"MLDSA87\0";
const GOVERNANCE_MLDSA87_LEVEL_V1: u32 = 87;

#[derive(Debug, Clone)]
pub(crate) struct GovernanceVoteVerifierConfigV1 {
    pub scheme: GovernanceVoteVerifierScheme,
    pub mldsa_mode: Option<String>,
    pub mldsa87_pubkeys: Option<HashMap<u32, Vec<u8>>>,
    pub aoem_dll_path: Option<String>,
}

pub(crate) fn governance_vote_message_bytes_v1(vote: &GovernanceVote) -> Vec<u8> {
    let mut message = Vec::with_capacity(8 + 8 + 8 + 32 + 1);
    message.extend_from_slice(b"GOV_VOTE_V1:");
    message.extend_from_slice(&vote.proposal_id.to_le_bytes());
    message.extend_from_slice(&vote.proposal_height.to_le_bytes());
    message.extend_from_slice(&vote.proposal_digest);
    message.push(if vote.support { 1 } else { 0 });
    message
}

pub(crate) fn encode_mldsa87_vote_signature_envelope_v1(
    pubkey: &[u8],
    signature: &[u8],
) -> Result<Vec<u8>> {
    if pubkey.is_empty() {
        bail!("mldsa_pubkey is empty");
    }
    if signature.is_empty() {
        bail!("signature is empty");
    }
    if pubkey.len() > u16::MAX as usize {
        bail!("mldsa_pubkey too large: {}", pubkey.len());
    }
    if signature.len() > u16::MAX as usize {
        bail!("signature too large: {}", signature.len());
    }
    let mut out = Vec::with_capacity(
        GOVERNANCE_MLDSA87_ENVELOPE_MAGIC_V1.len() + 2 + 2 + pubkey.len() + signature.len(),
    );
    out.extend_from_slice(GOVERNANCE_MLDSA87_ENVELOPE_MAGIC_V1);
    out.extend_from_slice(&(pubkey.len() as u16).to_le_bytes());
    out.extend_from_slice(&(signature.len() as u16).to_le_bytes());
    out.extend_from_slice(pubkey);
    out.extend_from_slice(signature);
    Ok(out)
}

fn decode_mldsa87_vote_signature_envelope_v1(raw: &[u8]) -> Result<(&[u8], &[u8])> {
    let min = GOVERNANCE_MLDSA87_ENVELOPE_MAGIC_V1.len() + 2 + 2;
    if raw.len() < min {
        bail!("mldsa87 signature envelope too short");
    }
    if &raw[..GOVERNANCE_MLDSA87_ENVELOPE_MAGIC_V1.len()] != GOVERNANCE_MLDSA87_ENVELOPE_MAGIC_V1 {
        bail!("invalid mldsa87 signature envelope magic");
    }
    let mut offset = GOVERNANCE_MLDSA87_ENVELOPE_MAGIC_V1.len();
    let pubkey_len = u16::from_le_bytes([raw[offset], raw[offset + 1]]) as usize;
    offset += 2;
    let signature_len = u16::from_le_bytes([raw[offset], raw[offset + 1]]) as usize;
    offset += 2;
    if pubkey_len == 0 || signature_len == 0 {
        bail!("mldsa87 signature envelope has empty pubkey or signature");
    }
    if raw.len() != offset + pubkey_len + signature_len {
        bail!("mldsa87 signature envelope length mismatch");
    }
    let pubkey = &raw[offset..offset + pubkey_len];
    let signature = &raw[offset + pubkey_len..];
    Ok((pubkey, signature))
}

fn governance_mldsa_mode_v1(config: &GovernanceVoteVerifierConfigV1) -> String {
    config
        .mldsa_mode
        .clone()
        .or_else(|| std::env::var("NOVOVM_GOVERNANCE_MLDSA_MODE").ok())
        .unwrap_or_else(|| "disabled".to_string())
        .trim()
        .to_ascii_lowercase()
}

fn install_legacy_aoem_env_alias_v1(config: Option<&GovernanceVoteVerifierConfigV1>) {
    if std::env::var_os("NOVOVM_AOEM_DLL").is_some() || std::env::var_os("AOEM_DLL").is_some() {
        return;
    }
    if let Some(path) = config.and_then(|cfg| cfg.aoem_dll_path.as_deref()) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            std::env::set_var("NOVOVM_AOEM_DLL", trimmed);
            return;
        }
    }
    if let Ok(path) = std::env::var("NOVOVM_AOEM_FFI_LIB_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            std::env::set_var("NOVOVM_AOEM_DLL", trimmed);
        }
    }
}

fn parse_governance_mldsa87_pubkeys_from_raw_v1(raw: &str) -> Result<HashMap<u32, Vec<u8>>> {
    let mut out = HashMap::new();
    for token in raw.split(',') {
        let entry = token.trim();
        if entry.is_empty() {
            continue;
        }
        let mut parts = entry.splitn(2, ':');
        let id_raw = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("invalid mldsa pubkey mapping entry: {}", entry))?;
        let pubkey_hex = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("invalid mldsa pubkey mapping entry: {}", entry))?;
        let voter_id = id_raw
            .trim()
            .parse::<u32>()
            .with_context(|| format!("invalid mldsa voter id in mapping: {}", id_raw.trim()))?;
        let pubkey = decode_hex_bytes_v1(pubkey_hex.trim(), "mldsa_pubkey")?;
        out.insert(voter_id, pubkey);
    }
    if out.is_empty() {
        bail!("NOVOVM_GOVERNANCE_MLDSA87_PUBKEYS resolved to empty mapping");
    }
    Ok(out)
}

fn parse_governance_mldsa87_pubkeys_from_config_or_env_v1(
    config: &GovernanceVoteVerifierConfigV1,
) -> Result<HashMap<u32, Vec<u8>>> {
    match config.mldsa87_pubkeys.clone() {
        Some(pubkeys) => Ok(pubkeys),
        None => {
            let raw = std::env::var("NOVOVM_GOVERNANCE_MLDSA87_PUBKEYS").map_err(|_| {
                anyhow::anyhow!(
                    "NOVOVM_GOVERNANCE_MLDSA87_PUBKEYS is required when NOVOVM_GOVERNANCE_VOTE_VERIFIER=mldsa87 and NOVOVM_GOVERNANCE_MLDSA_MODE=aoem_ffi"
                )
            })?;
            parse_governance_mldsa87_pubkeys_from_raw_v1(raw.as_str())
        }
    }
}

fn decode_hex_bytes_v1(raw: &str, field: &str) -> Result<Vec<u8>> {
    let normalized = raw
        .trim()
        .strip_prefix("0x")
        .or_else(|| raw.trim().strip_prefix("0X"))
        .unwrap_or(raw.trim());
    if normalized.is_empty() {
        bail!("{field} is empty");
    }
    if !normalized.len().is_multiple_of(2) {
        bail!("{field} must be even-length hex");
    }
    let mut out = Vec::with_capacity(normalized.len() / 2);
    for pair in normalized.as_bytes().chunks_exact(2) {
        let hex =
            std::str::from_utf8(pair).with_context(|| format!("{field} contains invalid utf8"))?;
        let byte = u8::from_str_radix(hex, 16)
            .with_context(|| format!("{field} contains invalid hex byte {}", hex))?;
        out.push(byte);
    }
    Ok(out)
}

struct AoemAutoMldsa87GovernanceVoteVerifierV1 {
    voter_pubkeys: HashMap<u32, Vec<u8>>,
}

impl AoemAutoMldsa87GovernanceVoteVerifierV1 {
    fn decode_vote_parts<'a>(
        &self,
        vote: &'a GovernanceVote,
    ) -> Result<(&'a [u8], &'a [u8]), BFTError> {
        let (pubkey, signature) = decode_mldsa87_vote_signature_envelope_v1(&vote.signature)
            .map_err(|e| BFTError::InvalidSignature(format!("invalid mldsa87 envelope: {}", e)))?;
        let expected_pubkey = self.voter_pubkeys.get(&vote.voter_id).ok_or_else(|| {
            BFTError::InvalidSignature(format!(
                "missing registered mldsa87 pubkey for voter {}",
                vote.voter_id
            ))
        })?;
        if expected_pubkey.as_slice() != pubkey {
            return Err(BFTError::InvalidSignature(format!(
                "mldsa87 pubkey mismatch for voter {}",
                vote.voter_id
            )));
        }
        Ok((pubkey, signature))
    }
}

impl GovernanceVoteVerifier for AoemAutoMldsa87GovernanceVoteVerifierV1 {
    fn name(&self) -> &'static str {
        "mldsa87_aoem_auto"
    }

    fn scheme(&self) -> GovernanceVoteVerifierScheme {
        GovernanceVoteVerifierScheme::MlDsa87
    }

    fn verify(
        &self,
        vote: &GovernanceVote,
        _key: &ed25519_dalek::VerifyingKey,
    ) -> std::result::Result<(), BFTError> {
        let (pubkey, signature) = self.decode_vote_parts(vote)?;
        install_legacy_aoem_env_alias_v1(None);
        let message = governance_vote_message_bytes_v1(vote);
        let valid = aoem_bindings::mldsa_verify_v1_auto(
            GOVERNANCE_MLDSA87_LEVEL_V1,
            pubkey,
            message.as_slice(),
            signature,
        )
        .map_err(|e| BFTError::InvalidSignature(format!("aoem auto mldsa verify failed: {}", e)))?
        .ok_or_else(|| {
            BFTError::InvalidSignature(format!(
                "aoem auto mldsa verify unavailable at {}",
                default_host_dll_path().display()
            ))
        })?;
        if !valid {
            return Err(BFTError::InvalidSignature(
                "aoem auto mldsa verify returned invalid".to_string(),
            ));
        }
        Ok(())
    }

    fn verify_batch_with_report(
        &self,
        inputs: &[GovernanceVoteVerificationInput<'_>],
    ) -> std::result::Result<Vec<GovernanceVoteVerificationReport>, BFTError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let mut pubkeys = Vec::with_capacity(inputs.len());
        let mut signatures = Vec::with_capacity(inputs.len());
        let mut messages = Vec::with_capacity(inputs.len());
        let mut voter_ids = Vec::with_capacity(inputs.len());
        for input in inputs {
            let (pubkey, signature) = self.decode_vote_parts(input.vote)?;
            pubkeys.push(pubkey.to_vec());
            signatures.push(signature.to_vec());
            messages.push(governance_vote_message_bytes_v1(input.vote));
            voter_ids.push(input.vote.voter_id);
        }
        let items: Vec<AoemMldsaVerifyItemRef<'_>> = (0..inputs.len())
            .map(|idx| AoemMldsaVerifyItemRef {
                level: GOVERNANCE_MLDSA87_LEVEL_V1,
                pubkey: pubkeys[idx].as_slice(),
                message: messages[idx].as_slice(),
                signature: signatures[idx].as_slice(),
            })
            .collect();
        install_legacy_aoem_env_alias_v1(None);
        let batch = aoem_bindings::mldsa_verify_batch_v1_auto(items.as_slice()).map_err(|e| {
            BFTError::InvalidSignature(format!("aoem auto mldsa verify batch failed: {}", e))
        })?;
        let Some(results) = batch else {
            let mut out = Vec::with_capacity(inputs.len());
            for input in inputs {
                self.verify(input.vote, input.key)?;
                out.push(GovernanceVoteVerificationReport {
                    verifier_name: self.name(),
                    scheme: self.scheme(),
                });
            }
            return Ok(out);
        };
        if results.len() != inputs.len() {
            return Err(BFTError::InvalidSignature(format!(
                "aoem auto mldsa verify batch result size mismatch: expected {} got {}",
                inputs.len(),
                results.len()
            )));
        }
        for (idx, valid) in results.iter().enumerate() {
            if *valid {
                continue;
            }
            return Err(BFTError::InvalidSignature(format!(
                "aoem auto mldsa verify batch returned invalid for voter {}",
                voter_ids[idx]
            )));
        }
        Ok(inputs
            .iter()
            .map(|_| GovernanceVoteVerificationReport {
                verifier_name: self.name(),
                scheme: self.scheme(),
            })
            .collect())
    }
}

fn build_aoem_auto_mldsa87_vote_verifier_v1(
    config: &GovernanceVoteVerifierConfigV1,
) -> Result<Arc<dyn GovernanceVoteVerifier>> {
    install_legacy_aoem_env_alias_v1(Some(config));
    if !default_host_dll_path().exists() {
        bail!(
            "AOEM host DLL not found at {}",
            default_host_dll_path().display()
        );
    }
    let mut voter_pubkeys = parse_governance_mldsa87_pubkeys_from_config_or_env_v1(config)?;
    for (voter, pubkey) in &voter_pubkeys {
        if pubkey.is_empty() {
            bail!("registered mldsa87 pubkey is empty for voter {}", voter);
        }
    }
    voter_pubkeys.shrink_to_fit();
    Ok(Arc::new(AoemAutoMldsa87GovernanceVoteVerifierV1 {
        voter_pubkeys,
    }))
}

pub(crate) fn apply_governance_vote_verifier_v1(
    engine: &BFTEngine,
    config: &GovernanceVoteVerifierConfigV1,
) -> Result<()> {
    match config.scheme {
        GovernanceVoteVerifierScheme::Ed25519 => engine
            .set_governance_vote_verifier_by_scheme(config.scheme)
            .map_err(|e| anyhow::anyhow!("{}", e)),
        GovernanceVoteVerifierScheme::MlDsa87 => match governance_mldsa_mode_v1(config).as_str() {
            "aoem_ffi" => {
                let custom = build_aoem_auto_mldsa87_vote_verifier_v1(config)?;
                engine.set_governance_vote_verifier(custom);
                Ok(())
            }
            "disabled" => bail!(
                "unsupported governance vote verifier: mldsa87 (disabled-by-policy, set NOVOVM_GOVERNANCE_MLDSA_MODE=aoem_ffi to enable)"
            ),
            other => bail!(
                "invalid NOVOVM_GOVERNANCE_MLDSA_MODE={} (valid: disabled, aoem_ffi)",
                other
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mldsa87_signature_envelope_roundtrip_v1() {
        let pubkey = vec![0x11u8; 7];
        let signature = vec![0x22u8; 11];
        let encoded = encode_mldsa87_vote_signature_envelope_v1(&pubkey, &signature)
            .expect("encode mldsa envelope");
        let (decoded_pubkey, decoded_signature) =
            decode_mldsa87_vote_signature_envelope_v1(encoded.as_slice())
                .expect("decode mldsa envelope");
        assert_eq!(decoded_pubkey, pubkey.as_slice());
        assert_eq!(decoded_signature, signature.as_slice());
    }
}
