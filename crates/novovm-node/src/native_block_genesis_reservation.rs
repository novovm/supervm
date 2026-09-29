//! Ledger-only reservation. NOT genesis activation or AOEM namespace validation.
use super::*;
#[path = "native_block_genesis_manifest.rs"]
mod manifest;
pub use manifest::NovNativeFreshFinalityProofV1;
pub use manifest::NovNativeFreshPromotionIntentV1;

pub(super) const RESERVED_SCHEMA: &str = "novovm-native-block-ledger/v1+genesis-reserved-v1";
const KEY_INTENT: &[u8] = b"native_block_ledger/v1/genesis/reservation";
const KEY_PIN: &[u8] = b"native_block_ledger/v1/genesis/reservation-pin";

pub(super) fn is_reserved_schema(raw: &[u8]) -> bool {
    raw == RESERVED_SCHEMA.as_bytes()
        || raw == manifest::MANIFEST_SCHEMA.as_bytes()
        || is_candidate_schema(raw)
}

pub(super) fn is_candidate_schema(raw: &[u8]) -> bool {
    raw == manifest::CANDIDATES_SCHEMA.as_bytes()
        || raw == manifest::PROMOTION_SCHEMA.as_bytes()
        || manifest::is_published_schema(raw)
}

pub(super) fn has_reservation_evidence(db: &DB) -> Result<bool> {
    Ok(db.get(KEY_INTENT)?.is_some()
        || db.get(KEY_PIN)?.is_some()
        || manifest::has_manifest_evidence(db)?)
}

/// Explicit commitments supplied by the future genesis coordinator. These bind
/// a local initialization attempt; they are not proof of operator authorization,
/// empty AOEM storage, an executed initial state or a finalized genesis block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeFreshGenesisReservationV1 {
    pub chain_id: u64,
    pub genesis_config_commitment: [u8; 32],
    pub initial_state_root: [u8; 32],
    pub validator_set_hash: [u8; 32],
    pub protocol_config_commitment: [u8; 32],
    /// Local AOEM domain, deliberately not part of the shared genesis hash.
    pub namespace_digest: [u8; 32],
}

impl NovNativeFreshGenesisReservationV1 {
    fn validate(&self) -> Result<()> {
        if self.chain_id == 0
            || [
                self.genesis_config_commitment,
                self.initial_state_root,
                self.validator_set_hash,
                self.protocol_config_commitment,
                self.namespace_digest,
            ]
            .contains(&[0; 32])
        {
            bail!("fresh genesis reservation requires nonzero chain and commitments");
        }
        Ok(())
    }

    fn pin(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"novovm-fresh-genesis-reservation-v1\0");
        hash.update(self.chain_id.to_be_bytes());
        hash.update(self.genesis_config_commitment);
        hash.update(self.initial_state_root);
        hash.update(self.validator_set_hash);
        hash.update(self.protocol_config_commitment);
        hash.update(self.namespace_digest);
        hash.finalize().into()
    }
}

impl NovNativeBlockLedgerV1 {
    /// Reserve an empty ledger, or verify an exact retry after restart. This
    /// library-only entrypoint never publishes state and has no CLI/RPC caller.
    /// The marker intentionally prevents ordinary/older writers from opening
    /// the reserved ledger until a future explicit recovery coordinator exists.
    /// Existing test ledgers are rejected, never migrated or cleared.
    pub fn reserve_fresh_genesis_v1(
        path: &Path,
        requested: &NovNativeFreshGenesisReservationV1,
    ) -> Result<()> {
        requested.validate()?;
        let ledger = Self::open_inner_v1(path, true)?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("NOV native block ledger write lock is poisoned"))?;
        let schema = ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .context("missing ledger schema")?;
        if schema.as_slice() == RESERVED_SCHEMA.as_bytes() {
            let stored = read_json_v1::<NovNativeFreshGenesisReservationV1>(
                &ledger.db,
                KEY_INTENT,
                "fresh genesis reservation",
            )?
            .context("fresh genesis reservation missing")?;
            stored.validate()?;
            if &stored != requested || ledger.db.get(KEY_PIN)?.as_deref() != Some(&stored.pin()[..])
            {
                bail!("fresh genesis reservation configuration or immutable pin mismatch");
            }
            for entry in ledger.db.iterator(rocksdb::IteratorMode::Start) {
                let (key, _) = entry?;
                if ![KEY_SCHEMA_V1, KEY_INTENT, KEY_PIN].contains(&key.as_ref()) {
                    bail!("fresh genesis reserved ledger contains unexpected state");
                }
            }
            return Ok(());
        }
        if schema.as_slice() != NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1.as_bytes() {
            bail!("fresh genesis requires an unused ledger schema");
        }
        for entry in ledger.db.iterator(rocksdb::IteratorMode::Start) {
            let (key, _) = entry?;
            if key.as_ref() != KEY_SCHEMA_V1 {
                bail!("fresh genesis refuses nonempty ledger; existing data preserved");
            }
        }
        let mut batch = RocksDbWriteBatch::default();
        batch.put(KEY_SCHEMA_V1, RESERVED_SCHEMA.as_bytes());
        put_json_v1(
            &mut batch,
            KEY_INTENT,
            requested,
            "fresh genesis reservation",
        )?;
        batch.put(KEY_PIN, requested.pin());
        write_sync_v1(&ledger.db, batch).context("persist fresh genesis reservation")
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::TestLedgerV1;
    use super::*;

    fn request() -> NovNativeFreshGenesisReservationV1 {
        NovNativeFreshGenesisReservationV1 {
            chain_id: 99,
            genesis_config_commitment: [1; 32],
            initial_state_root: [2; 32],
            validator_set_hash: [3; 32],
            protocol_config_commitment: [4; 32],
            namespace_digest: [5; 32],
        }
    }

    #[test]
    fn existing_ledger_open_does_not_reenter_candidate_write_lock() {
        let fixture = TestLedgerV1::new("genesis-open-lock");
        let guard = fixture.ledger().lock_writes_v1().unwrap();
        let path = fixture.path.clone();
        let (send, receive) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let opened = NovNativeBlockLedgerV1::open(&path);
            send.send(opened.is_ok()).unwrap();
        });
        let before_unlock = receive.recv_timeout(std::time::Duration::from_secs(5));
        // Always release/join, so a regression fails rather than hanging CI.
        drop(guard);
        worker.join().unwrap();
        assert_eq!(before_unlock, Ok(true));
    }

    #[test]
    fn fresh_genesis_reservation_restarts_and_fences_existing_handles() {
        let mut fixture = TestLedgerV1::new("genesis-reserve");
        let req = request();
        NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&fixture.path, &req).unwrap();
        assert!(fixture.ledger().lock_writes_v1().is_err());
        assert!(NovNativeBlockLedgerV1::open(&fixture.path).is_err());
        assert!(NovNativeBlockLedgerV1::open_existing_read_only(&fixture.path).is_err());
        fixture.ledger.take();
        NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&fixture.path, &req).unwrap();
        let mut changed = req.clone();
        changed.namespace_digest = [6; 32];
        assert!(NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&fixture.path, &changed).is_err());
        NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&fixture.path, &req).unwrap();
    }

    #[test]
    fn fresh_genesis_reservation_preserves_occupied_and_orphaned_data() {
        for key in [b"test-ledger-state".as_slice(), KEY_INTENT, KEY_PIN] {
            let fixture = TestLedgerV1::new("genesis-occupied");
            fixture.ledger().db.put(key, b"preserve-me").unwrap();
            assert!(
                NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&fixture.path, &request())
                    .is_err()
            );
            assert_eq!(
                fixture.ledger().db.get(key).unwrap().unwrap(),
                b"preserve-me"
            );
            assert_eq!(
                fixture.ledger().db.get(KEY_SCHEMA_V1).unwrap().unwrap(),
                NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1.as_bytes()
            );
        }
    }

    #[test]
    fn fresh_genesis_reservation_rejects_missing_tampered_and_downgraded_evidence() {
        for key in [KEY_INTENT, KEY_PIN, KEY_SCHEMA_V1] {
            let fixture = TestLedgerV1::new("genesis-missing");
            NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&fixture.path, &request()).unwrap();
            fixture.ledger().db.delete(key).unwrap();
            assert!(
                NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&fixture.path, &request())
                    .is_err()
            );
        }
        let fixture = TestLedgerV1::new("genesis-tamper");
        NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&fixture.path, &request()).unwrap();
        fixture.ledger().db.put(KEY_PIN, [9; 32]).unwrap();
        assert!(
            NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&fixture.path, &request()).is_err()
        );
        fixture
            .ledger()
            .db
            .put(KEY_SCHEMA_V1, NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1.as_bytes())
            .unwrap();
        assert!(fixture.ledger().lock_writes_v1().is_err());
        assert!(NovNativeBlockLedgerV1::open(&fixture.path).is_err());
        assert!(
            NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&fixture.path, &request()).is_err()
        );
    }

    #[test]
    fn fresh_genesis_reservation_invalid_request_does_not_touch_storage() {
        let fixture = TestLedgerV1::new("genesis-invalid");
        let target = fixture.path.join("must-not-exist");
        let mut req = request();
        req.chain_id = 0;
        assert!(NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&target, &req).is_err());
        assert!(!target.exists());
    }

    #[test]
    fn fresh_genesis_reservation_competing_initializers_have_one_winner() {
        let fixture = TestLedgerV1::new("genesis-race");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let attempts: Vec<_> = (1..=2)
            .map(|id| {
                let path = fixture.path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut req = request();
                    req.genesis_config_commitment = [id; 32];
                    barrier.wait();
                    let accepted =
                        NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&path, &req).is_ok();
                    (req, accepted)
                })
            })
            .collect();
        let results: Vec<_> = attempts
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|(_, accepted)| *accepted).count(), 1);
        for (req, accepted) in results {
            assert_eq!(
                NovNativeBlockLedgerV1::reserve_fresh_genesis_v1(&fixture.path, &req).is_ok(),
                accepted
            );
        }
    }
}
