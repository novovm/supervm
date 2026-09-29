//! Durable approved inputs only. No AOEM write or genesis activation.
use super::*;
use crate::tx_ingress::fresh_genesis::FreshGenesisConfigV1;

pub(super) const MANIFEST_SCHEMA: &str =
    "novovm-native-block-ledger/v1+genesis-manifest-reserved-v1";
const KEY_MANIFEST: &[u8] = b"native_block_ledger/v1/genesis/manifest";
const KEY_MANIFEST_PIN: &[u8] = b"native_block_ledger/v1/genesis/manifest-pin";

pub(super) fn has_manifest_evidence(db: &DB) -> Result<bool> {
    Ok(db.get(KEY_MANIFEST)?.is_some() || db.get(KEY_MANIFEST_PIN)?.is_some())
}

fn manifest_pin(bytes: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"novovm-fresh-genesis-manifest-archive-v1\0");
    hash.update(bytes);
    hash.finalize().into()
}

fn load_verified(
    ledger: &NovNativeBlockLedgerV1,
    expected: [u8; 32],
    namespace: [u8; 32],
) -> Result<FreshGenesisConfigV1> {
    if ledger.db.get(KEY_SCHEMA_V1)?.as_deref() != Some(MANIFEST_SCHEMA.as_bytes()) {
        bail!("complete genesis manifest reservation is required; no implicit upgrade");
    }
    let bytes = ledger
        .db
        .get(KEY_MANIFEST)?
        .context("genesis manifest missing")?;
    if ledger.db.get(KEY_MANIFEST_PIN)?.as_deref() != Some(&manifest_pin(&bytes)[..]) {
        bail!("genesis manifest archive pin mismatch");
    }
    let config = FreshGenesisConfigV1::from_json(&bytes)?;
    let rebuilt = config.compile()?.reservation(expected, namespace)?;
    let stored = read_json_v1::<NovNativeFreshGenesisReservationV1>(
        &ledger.db,
        KEY_INTENT,
        "genesis manifest reservation",
    )?
    .context("genesis manifest reservation missing")?;
    stored.validate()?;
    if stored != rebuilt || ledger.db.get(KEY_PIN)?.as_deref() != Some(&stored.pin()[..]) {
        bail!("genesis manifest does not reconstruct the pinned reservation");
    }
    for entry in ledger.db.iterator(rocksdb::IteratorMode::Start) {
        let (key, _) = entry?;
        if ![
            KEY_SCHEMA_V1,
            KEY_INTENT,
            KEY_PIN,
            KEY_MANIFEST,
            KEY_MANIFEST_PIN,
        ]
        .contains(&key.as_ref())
        {
            bail!("genesis manifest reservation contains unexpected ledger state");
        }
    }
    Ok(config)
}

impl NovNativeBlockLedgerV1 {
    /// Atomically archive the complete explicit configuration with its computed
    /// reservation. An existing hash-only reservation cannot be upgraded: missing
    /// historical inputs must never be manufactured during recovery.
    pub fn reserve_fresh_genesis_config_v1(
        path: &Path,
        config: &FreshGenesisConfigV1,
        expected: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<()> {
        let request = config.compile()?.reservation(expected, namespace)?;
        let bytes = serde_json::to_vec(config)?;
        FreshGenesisConfigV1::from_json(&bytes)?; // enforce durable byte bound before IO
        let ledger = Self::open_inner_v1(path, true)?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("genesis ledger lock poisoned"))?;
        let schema = ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .context("genesis ledger schema missing")?;
        if schema.as_slice() == MANIFEST_SCHEMA.as_bytes() {
            load_verified(&ledger, expected, namespace)?;
            return Ok(()); // preserve the original archived representation
        }
        if schema.as_slice() != NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1.as_bytes() {
            bail!("fresh genesis manifest requires an unused ledger");
        }
        for entry in ledger.db.iterator(rocksdb::IteratorMode::Start) {
            let (key, _) = entry?;
            if key.as_ref() != KEY_SCHEMA_V1 {
                bail!("fresh genesis manifest refuses occupied ledger; data preserved");
            }
        }
        let mut batch = RocksDbWriteBatch::default();
        batch.put(KEY_SCHEMA_V1, MANIFEST_SCHEMA.as_bytes());
        put_json_v1(
            &mut batch,
            KEY_INTENT,
            &request,
            "genesis manifest reservation",
        )?;
        batch.put(KEY_PIN, request.pin());
        batch.put(KEY_MANIFEST, &bytes);
        batch.put(KEY_MANIFEST_PIN, manifest_pin(&bytes));
        write_sync_v1(&ledger.db, batch)?;
        load_verified(&ledger, expected, namespace)?;
        Ok(())
    }

    /// Read-only recovery of approved inputs; does not create a DB, repair lost
    /// evidence, publish AOEM state or clear the ordinary-writer fence.
    pub fn load_fresh_genesis_config_v1(
        path: &Path,
        expected: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<Option<FreshGenesisConfigV1>> {
        let Some(ledger) = Self::open_existing_read_only_inner_v1(path, true)? else {
            return Ok(None);
        };
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("genesis ledger lock poisoned"))?;
        Ok(Some(load_verified(&ledger, expected, namespace)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_block_ledger::tests::TestLedgerV1;
    use crate::tx_ingress::fresh_genesis::{
        GenesisAllocationV1, GenesisValidatorV1, GENESIS_SCHEMA_V1,
    };

    fn config() -> FreshGenesisConfigV1 {
        FreshGenesisConfigV1 {
            schema: GENESIS_SCHEMA_V1.into(),
            chain_id: 97,
            timestamp_unix_ms: 1900000000000,
            protocol_config_commitment: [1; 32],
            allocations: vec![GenesisAllocationV1 {
                account: [8; 20],
                nov: "123".into(),
            }],
            total_initial_nov: "123".into(),
            validators: vec![GenesisValidatorV1 {
                public_key: ed25519_dalek::SigningKey::from_bytes(&[1; 32])
                    .verifying_key()
                    .to_bytes(),
                weight: 1,
            }],
        }
    }

    #[test]
    fn genesis_manifest_restart_rebuilds_exact_state_and_preserves_fence() {
        let mut fixture = TestLedgerV1::new("manifest-restart");
        let config = config();
        let compiled = config.compile().unwrap();
        let pin = compiled.config_commitment();
        NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
            &fixture.path,
            &config,
            pin,
            [3; 32],
        )
        .unwrap();
        fixture.ledger.take();
        let restored =
            NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&fixture.path, pin, [3; 32])
                .unwrap()
                .unwrap();
        assert_eq!(
            restored.compile().unwrap().initial_store(),
            compiled.initial_store()
        );
        NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
            &fixture.path,
            &restored,
            pin,
            [3; 32],
        )
        .unwrap();
        assert!(NovNativeBlockLedgerV1::open(&fixture.path).is_err());
        assert!(NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(
            &fixture.path,
            [9; 32],
            [3; 32]
        )
        .is_err());
        assert!(
            NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&fixture.path, pin, [4; 32])
                .is_err()
        );
    }

    #[test]
    fn genesis_manifest_missing_evidence_is_never_repaired_by_retry() {
        for key in [
            KEY_MANIFEST,
            KEY_MANIFEST_PIN,
            KEY_INTENT,
            KEY_PIN,
            KEY_SCHEMA_V1,
        ] {
            let fixture = TestLedgerV1::new("manifest-loss");
            let config = config();
            let pin = config.compile().unwrap().config_commitment();
            NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
                &fixture.path,
                &config,
                pin,
                [3; 32],
            )
            .unwrap();
            fixture.ledger().db.delete(key).unwrap();
            assert!(NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(
                &fixture.path,
                pin,
                [3; 32]
            )
            .is_err());
            assert!(NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
                &fixture.path,
                &config,
                pin,
                [3; 32]
            )
            .is_err());
            assert!(fixture.ledger().db.get(key).unwrap().is_none());
        }
    }

    #[test]
    fn genesis_manifest_rejects_tampering_even_with_recomputed_archive_checksum() {
        let fixture = TestLedgerV1::new("manifest-tamper");
        let mut config = config();
        let pin = config.compile().unwrap().config_commitment();
        NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
            &fixture.path,
            &config,
            pin,
            [3; 32],
        )
        .unwrap();
        config.timestamp_unix_ms += 1;
        let bytes = serde_json::to_vec(&config).unwrap();
        fixture.ledger().db.put(KEY_MANIFEST, &bytes).unwrap();
        fixture
            .ledger()
            .db
            .put(KEY_MANIFEST_PIN, manifest_pin(&bytes))
            .unwrap();
        assert!(
            NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&fixture.path, pin, [3; 32])
                .is_err()
        );
        fixture
            .ledger()
            .db
            .put(KEY_SCHEMA_V1, NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1.as_bytes())
            .unwrap();
        assert!(NovNativeBlockLedgerV1::open(&fixture.path).is_err());
    }

    #[test]
    fn genesis_manifest_read_missing_path_has_no_side_effects() {
        let fixture = TestLedgerV1::new("manifest-absent");
        let path = fixture.path.join("absent");
        assert!(
            NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&path, [1; 32], [3; 32])
                .unwrap()
                .is_none()
        );
        assert!(!path.exists());
        let config = config();
        assert!(NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
            &path, &config, [9; 32], [3; 32]
        )
        .is_err());
        assert!(!path.exists());
    }

    #[test]
    fn genesis_manifest_refuses_occupied_or_hash_only_ledgers() {
        let config = config();
        let compiled = config.compile().unwrap();
        let pin = compiled.config_commitment();
        let fixture = TestLedgerV1::new("manifest-occupied");
        fixture
            .ledger()
            .db
            .put(b"existing-test-history", b"untouched")
            .unwrap();
        assert!(NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
            &fixture.path,
            &config,
            pin,
            [3; 32]
        )
        .is_err());
        assert_eq!(
            fixture
                .ledger()
                .db
                .get(b"existing-test-history")
                .unwrap()
                .unwrap(),
            b"untouched"
        );
        assert!(fixture.ledger().db.get(KEY_MANIFEST).unwrap().is_none());
        let fixture = TestLedgerV1::new("manifest-no-upgrade");
        NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(
            &fixture.path,
            &compiled.reservation(pin, [3; 32]).unwrap(),
        )
        .unwrap();
        assert!(NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
            &fixture.path,
            &config,
            pin,
            [3; 32]
        )
        .is_err());
        assert!(fixture.ledger().db.get(KEY_MANIFEST).unwrap().is_none());
    }
}
