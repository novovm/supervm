//! Validate the existing offline configuration against its original storage owners.
use super::*;

impl NovNativeSealServiceConfigV1 {
    pub(super) fn check_startup_paths(&self, params: &serde_json::Value) -> Result<()> {
        let paths = crate::tx_ingress::native_persistence_write_paths_v1(params);
        let ledger = paths
            .iter()
            .find(|(label, _)| *label == "native block ledger")
            .context("startup ledger path missing")?
            .1
            .clone();
        let writes = paths
            .into_iter()
            .filter(|(label, _)| *label != "native block ledger")
            .map(|(_, path)| path)
            .collect::<Vec<_>>();
        crate::native_block_seal::service_paths::validate_service_paths_v1(
            self,
            &ledger,
            &writes,
            &[],
        )
    }
}
