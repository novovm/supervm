//! Original key domains shared by UCA identity and asset state.
use anyhow::{ensure, Result};
use novovm_protocol::uca_spend::MAX_UCA_ID_BYTES;

const DOMAIN: &[u8] = b"novovm/asset-spend-state/v2/";

pub(super) fn valid_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= MAX_UCA_ID_BYTES
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b)),
        "invalid UCA state identity"
    );
    Ok(())
}

pub(super) fn domain(chain: u64, genesis: &[u8; 32], kind: &[u8]) -> Result<Vec<u8>> {
    ensure!(chain != 0 && *genesis != [0; 32], "empty UCA state domain");
    Ok([DOMAIN, &chain.to_be_bytes(), genesis, kind].concat())
}
