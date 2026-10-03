//! Isolated dual-node gate fixtures, never transaction-time funding.
use super::*;
use novovm_node::tx_ingress::{
    load_nov_native_execution_store_v1, native_host_projection_bootstrap_anchor_commitment_v1,
    save_nov_native_execution_store_v1, NovNativeExecutionStoreV1,
};
use sha2::{Digest, Sha256};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct RestoreEnv(Vec<(&'static str, Option<std::ffi::OsString>)>);
impl Drop for RestoreEnv {
    fn drop(&mut self) {
        for (key, value) in &self.0 {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

// Called in the single-threaded gate parent before child processes start, or
// after they have exited. Explicitly override inherited storage destinations.
fn with_store<T>(path: &Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let keys = [
        "NOVOVM_NATIVE_EXECUTION_STORE_BACKEND",
        NOV_NATIVE_EXECUTION_STORE_ROCKSDB_PATH_ENV,
    ];
    let _restore = RestoreEnv(
        keys.iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect(),
    );
    std::env::set_var(keys[0], "rocksdb");
    std::env::set_var(keys[1], append_path_suffix_v1(path, ".rocksdb"));
    f()
}

pub(super) fn allocation(chain_id: u64, count: u64) -> Result<NovNativeExecutionStoreV1> {
    if count == 0 || count > 100_000 {
        bail!("funded gate requires 1..=100000 transactions");
    }
    let mut store = NovNativeExecutionStoreV1::default();
    for identity in 1..=count {
        let mut hash = Sha256::new();
        hash.update(b"novovm-native-fixture-signing-seed/v1");
        hash.update(chain_id.to_le_bytes());
        hash.update(identity.to_le_bytes());
        let address = novovm_adapter_novovm::address_from_seed_v1(hash.finalize().into());
        let account = format!("0x{}", hex(&address));
        store.module_state.account_asset_balances.insert(
            account,
            std::collections::BTreeMap::from([
                ("USDT".into(), identity as u128),
                ("NOV".into(), 10_000),
            ]),
        );
    }
    Ok(store)
}

pub(super) fn prepare(path: &Path, chain_id: u64, node: u64, count: u64) -> Result<String> {
    let store = allocation(chain_id, count)?;
    for target in [
        path.to_path_buf(),
        append_path_suffix_v1(path, ".rocksdb"),
        append_path_suffix_v1(path, ".block-ledger.rocksdb"),
        path.with_extension("aoem-owned.rocksdb"),
        path.with_extension("aoem-persistence"),
    ] {
        if target.exists() {
            bail!(
                "refuse to fund an existing gate store: {}",
                target.display()
            );
        }
    }
    // Exclusive marker prevents a second initializer from overwriting this fixture.
    let _marker = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(append_path_suffix_v1(path, ".funded-fixture"))?;
    let namespace = format!("dual-node-gate-chain-{chain_id}-node-{node}");
    let mut hash = Sha256::new();
    hash.update(b"novovm-native-aoem-state-namespace-v1");
    hash.update(namespace.as_bytes());
    let anchor = native_host_projection_bootstrap_anchor_commitment_v1(
        &store,
        chain_id,
        &hex(&hash.finalize()),
    )?;
    with_store(path, || {
        save_nov_native_execution_store_v1(path, &store)?;
        if load_nov_native_execution_store_v1(path)? != store {
            bail!("funded fixture readback mismatch");
        }
        Ok(())
    })?;
    Ok(anchor)
}

pub(super) fn verify(path: &Path, count: u64) -> Result<serde_json::Value> {
    let store = with_store(path, || load_nov_native_execution_store_v1(path))?;
    verify_accounting(&store, count)
}

fn verify_accounting(store: &NovNativeExecutionStoreV1, count: u64) -> Result<serde_json::Value> {
    if count == 0 || count > 100_000 {
        bail!("funded gate requires 1..=100000 transactions");
    }
    let deposits: Vec<_> = store
        .receipts
        .values()
        .filter(|r| r.module == "treasury" && r.method == "deposit_reserve")
        .collect();
    if deposits.len() != count as usize || deposits.iter().any(|r| !r.status) {
        bail!(
            "funded gate requires {count} successful persisted deposit receipts, got {}",
            deposits.len()
        );
    }
    let expected = (count as u128) * (count as u128 + 1) / 2;
    let reserve = store
        .module_state
        .treasury_reserves
        .get("USDT")
        .copied()
        .unwrap_or(0);
    let remaining = store
        .module_state
        .account_asset_balances
        .values()
        .map(|v| v.get("USDT").copied().unwrap_or(0))
        .try_fold(0u128, |total, amount| total.checked_add(amount))
        .context("funded gate account balance sum overflow")?;
    if reserve != expected || remaining != 0 {
        bail!("funded gate conservation mismatch: reserve={reserve} expected={expected} remaining={remaining}");
    }
    Ok(
        serde_json::json!({"successful_deposits": count, "usdt_reserve": reserve.to_string(), "usdt_accounts_remaining": "0"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic snapshots exercise rejection logic only, not AOEM execution.
    fn successful_accounting_snapshot() -> NovNativeExecutionStoreV1 {
        let mut store = allocation(7, 2).unwrap();
        for assets in store.module_state.account_asset_balances.values_mut() {
            assets.insert("USDT".into(), 0);
        }
        store
            .module_state
            .treasury_reserves
            .insert("USDT".into(), 3);
        for identity in 1..=2 {
            let hash = format!("fixture-{identity}");
            let receipt = serde_json::from_value(serde_json::json!({
                "tx_hash": hash, "status": true, "target": "treasury",
                "module": "treasury", "method": "deposit_reserve",
                "settled_fee_nov": 0, "paid_asset": "NOV", "paid_amount": 0,
                "logs": [], "failure_reason": null, "fee_contract": "test-only"
            }))
            .unwrap();
            store.receipts.insert(hash, receipt);
        }
        store
    }

    #[test]
    fn accounting_rejects_incomplete_failed_or_unrelated_receipts() {
        let baseline = successful_accounting_snapshot();
        assert_eq!(
            verify_accounting(&baseline, 2).unwrap()["usdt_reserve"],
            "3"
        );
        assert!(verify_accounting(&baseline, 0).is_err());
        assert!(verify_accounting(&baseline, 100_001).is_err());
        let mut missing = baseline.clone();
        missing.receipts.pop_first();
        assert!(verify_accounting(&missing, 2).is_err());
        let mut failed = baseline.clone();
        failed.receipts.values_mut().next().unwrap().status = false;
        assert!(verify_accounting(&failed, 2).is_err());
        let mut unrelated = baseline.clone();
        unrelated.receipts.values_mut().next().unwrap().method = "other".into();
        assert!(verify_accounting(&unrelated, 2).is_err());
        let mut excess = baseline.clone();
        excess.receipts.insert(
            "extra".into(),
            baseline.receipts.values().next().unwrap().clone(),
        );
        assert!(verify_accounting(&excess, 2).is_err());
    }

    #[test]
    fn accounting_rejects_wrong_reserve_remaining_balance_and_overflow() {
        let baseline = successful_accounting_snapshot();
        for reserve in [0, 2, 4] {
            let mut wrong = baseline.clone();
            wrong
                .module_state
                .treasury_reserves
                .insert("USDT".into(), reserve);
            assert!(verify_accounting(&wrong, 2).is_err());
        }
        let mut remaining = baseline.clone();
        remaining
            .module_state
            .account_asset_balances
            .values_mut()
            .next()
            .unwrap()
            .insert("USDT".into(), 1);
        assert!(verify_accounting(&remaining, 2).is_err());
        let mut overflow = baseline;
        for assets in overflow.module_state.account_asset_balances.values_mut() {
            assets.insert("USDT".into(), u128::MAX);
        }
        assert!(verify_accounting(&overflow, 2)
            .unwrap_err()
            .to_string()
            .contains("overflow"));
    }

    #[test]
    fn allocation_is_bounded_deterministic_and_chain_specific() {
        assert!(allocation(7, 0).is_err());
        assert!(allocation(7, 100_001).is_err());
        let first = allocation(7, 8).unwrap();
        assert_eq!(first, allocation(7, 8).unwrap());
        assert_ne!(
            first.module_state.account_asset_balances,
            allocation(8, 8)
                .unwrap()
                .module_state
                .account_asset_balances
        );
        assert_eq!(first.module_state.account_asset_balances.len(), 8);
        assert_eq!(
            first
                .module_state
                .account_asset_balances
                .values()
                .map(|v| v["USDT"])
                .sum::<u128>(),
            36
        );
        assert!(first.receipts.is_empty());
        assert!(first.module_state.treasury_reserves.is_empty());
        assert!(first.module_state.protocol_config_commitment.is_empty());
    }

    #[test]
    fn initializer_refuses_existing_store_without_touching_it() {
        let path = temp_store_path_v1("refuse-existing-unit", 987654321);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        use std::io::Write;
        file.write_all(b"do not overwrite").unwrap();
        drop(file);
        assert!(prepare(&path, 987654321, 1, 8)
            .unwrap_err()
            .to_string()
            .contains("existing gate store"));
        assert_eq!(fs::read(&path).unwrap(), b"do not overwrite");
        fs::remove_file(path).unwrap();
    }
}
