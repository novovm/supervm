#[allow(clippy::too_many_arguments)]
fn exercise_successor_promotion_intent(
    path: &Path,
    params: &serde_json::Value,
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    competing: [u8; 32],
    pin: [u8; 32],
    proof: &crate::native_block_ledger::NovNativeFreshFinalityProofV1,
) {
    use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
    let ledger = nov_native_block_ledger_rocksdb_path_v1(path);
    let prepare = |candidate, proof| {
        workspace::prepare_successor_promotion_v1(chain, parent, candidate, pin, proof, params)
    };
    let mut invalid = proof.clone();
    if let Message::DecisionCertificateV3 { decision, .. } = &mut invalid.witness {
        decision.votes.truncate(2);
    }
    assert!(prepare(id, &invalid).is_err());
    let mut invalid = proof.clone();
    if let Message::DecisionCertificateV3 { decision, .. } = &mut invalid.witness {
        decision.prepare.subject.post_state_root[0] ^= 1;
    }
    assert!(prepare(id, &invalid).is_err());
    assert!(prepare(competing, proof).is_err());
    let key = b"native_block_ledger/v1/successor/promotion";
    let pin_key = b"native_block_ledger/v1/successor/promotion-pin";
    let db = rocksdb::DB::open_default(&ledger).unwrap();
    assert!(db.get(key).unwrap().is_none());
    assert!(db.get(pin_key).unwrap().is_none());
    drop(db);
    let commitment = prepare(id, proof).unwrap();
    assert_ne!(commitment, [0; 32]);
    assert_eq!(prepare(id, proof).unwrap(), commitment);
    assert!(prepare(competing, proof).is_err());
    assert!(workspace::abort_v1(chain, id, params).is_err());
    assert!(workspace::abort_v1(chain, competing, params).is_err());
    assert!(workspace::register_finalized_successor_v1(chain, parent, id, pin, params).is_err());
    assert!(workspace::with_verified_finalized_successor_v1(
        chain,
        parent,
        id,
        pin,
        params,
        |_| -> Result<()> { panic!("staged promotion reached signing callback") }
    )
    .is_err());
    // A committed marker requires all evidence; do not silently reconstruct it.
    for missing in [key.as_slice(), pin_key.as_slice()] {
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        let bytes = db.get(missing).unwrap().unwrap();
        db.delete(missing).unwrap();
        drop(db);
        assert!(prepare(id, proof).is_err());
        assert!(workspace::abort_v1(chain, id, params).is_err());
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        assert!(db.get(missing).unwrap().is_none());
        db.put(missing, bytes).unwrap(); // Explicit test-only restoration.
    }
    assert_eq!(prepare(id, proof).unwrap(), commitment);
    let db = rocksdb::DB::open_default(&ledger).unwrap();
    assert_eq!(db.get(pin_key).unwrap().unwrap(), commitment);
}
