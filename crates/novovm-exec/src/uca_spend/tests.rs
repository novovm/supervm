//! Real original verifier evidence and compact reattachment only; no asset state
//! or successful spend is manufactured by a signature-fact test.
use super::*;

#[test]
#[ignore = "requires explicit trusted AOEM library; real ML-DSA-65 fact binding and compact retention"]
fn asset_signature_fact_compacts_owned_crypto_and_rejects_all_transplants() -> Result<()> {
    let path = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .ok_or_else(|| anyhow::anyhow!("explicit trusted AOEM library required"))?;
    let facade = AoemExecFacade::open(std::path::Path::new(&path), Default::default())?;
    let runtime = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let key = runtime.generate_keypair()?;
    let signer = facade.asset_spend_signer_v2()?;
    let signed = signer.sign(
        AssetSpendIntentV2 {
            chain_id: 717,
            genesis: [1; 32],
            payer: vec![1; 20],
            recipient: vec![2; 20],
            asset: 1,
            amount: 30,
            max_fee: 50,
            nonce: 0,
            authorization_epoch: 1,
            spend_key_ref: spend_key_ref_v1(&key.public_key)?,
            not_before_height: 1,
            expires_at_height: 50,
            authority_update: None,
            recipient_creation: None,
        },
        &key.public_key,
        &key.expanded_secret_key,
    )?;
    let bytes = signed.encode()?;
    let checked = facade.asset_spend_verifier_v2()?.verify(signed.clone())?;
    let owned_bytes = std::mem::size_of::<SignatureCheckedAssetSpendV2>()
        + signed.intent.retained_heap_bytes()
        + signed.public_key.capacity()
        + signed.signature.capacity();
    let fact = checked.into_signature_fact()?;
    let compact_bytes =
        std::mem::size_of::<AssetSpendSignatureFactV2>() + fact.intent().retained_heap_bytes();
    assert!(owned_bytes - compact_bytes > 5_000);
    let restored = fact.reattach(signed.clone())?;
    assert_eq!(restored.signed().encode()?, bytes);
    assert_eq!(
        restored.check_supplied_state(None, &[0; 32], 2),
        Err(novovm_adapter_api::uca_spend_policy::SpendPolicyError::StateUnavailable)
    );
    for mutation in 0..13 {
        let mut changed = signed.clone();
        match mutation {
            0 => changed.intent.payer[0] ^= 1,
            1 => changed.intent.recipient[0] ^= 1,
            2 => changed.intent.chain_id += 1,
            3 => changed.intent.genesis[0] ^= 1,
            4 => changed.intent.amount += 1,
            5 => changed.intent.max_fee += 1,
            6 => changed.intent.nonce += 1,
            7 => changed.intent.authorization_epoch += 1,
            8 => changed.intent.spend_key_ref[0] ^= 1,
            9 => changed.intent.expires_at_height += 1,
            10 => changed.public_key[0] ^= 1,
            11 => changed.signature[0] ^= 1,
            _ => changed.signature.truncate(changed.signature.len() - 1),
        }
        assert!(fact.reattach(changed).is_err(), "mutation {mutation}");
    }
    let mut wrong_context = signed.clone();
    wrong_context.signature = runtime.sign(
        &key.public_key,
        &key.expanded_secret_key,
        &signed.intent.signing_bytes()?,
        b"not-the-asset-spend-context",
    )?;
    assert!(fact.reattach(wrong_context).is_err());
    let mut wrong_parameter = bytes.clone();
    wrong_parameter[5] = 87;
    assert!(SignedAssetSpendV2::decode(&wrong_parameter).is_err());
    let mut next_intent = signed.intent.clone();
    next_intent.nonce += 1;
    let next = signer.sign(next_intent, &key.public_key, &key.expanded_secret_key)?;
    let next_fact = facade
        .asset_spend_verifier_v2()?
        .verify(next.clone())?
        .into_signature_fact()?;
    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                assert!(fact.reattach(signed.clone()).is_ok());
                assert!(next_fact.reattach(next.clone()).is_ok());
                assert!(fact.reattach(next.clone()).is_err());
                assert!(next_fact.reattach(signed.clone()).is_err());
            });
        }
    });
    Ok(())
}
