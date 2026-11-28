//! Real-library component tests. The application profile is not activated in
//! the node service; neither execution nor persistence is publication/login.
use super::{
    application_effect_contract_v1, application_program_id_v1, application_receipt_codec_v1,
    ApplicationGrantBodyV1, ApplicationGrantInputV1, ApplicationGrantPlanV1,
    ExecutedApplicationGrantBatchV1,
};
use crate::native_pipeline::{
    business::{
        direct_nov_fee::DirectNovFeePolicy,
        uca_application::{ApplicationGrantAccessV1, ApplicationGrantObservationV1},
        uca_registration,
        uca_spend_state::identity_record_changes,
    },
    execution::plan::{
        executed_statement_commitment, ordered_receipt_commitment, BatchContext, PlanBudget,
    },
    persistence::{
        CandidateStore, OpenMode, PacketBudget, PreparedCandidate, StorageDomain, StoreConfig,
    },
    state::{
        frontier::{CaptureBudget, CaptureStep, OwnedStateInput},
        tree::{
            empty_root, read_state_value, stage_state_update, NodeHash, StagedStateUpdate,
            StateNodeReader,
        },
    },
};
use anyhow::{Context, Result};
use novovm_adapter_api::{
    uca_key_binding::derive_primary_key_ref_from_binding_v1,
    unified_account::{UcaAccount, UcaKeyAlgo, UcaKeyProofType, UcaPrimaryKeyBinding, UcaStatus},
};
use novovm_exec::{
    pq_signature::{AoemMldsaParameterSet, MldsaKeyPair},
    resident::{ComputeSession, StorageConfig},
    uca_application::{ApplicationGrantSignatureFactV1, ApplicationGrantVerifierV1},
    AoemExecFacade, AoemExecOpenOptions,
};
use novovm_protocol::{
    native_query::{ApplicationGrantStateV1, IdentityStatusV1, UcaIdentityIdV1},
    uca_application::{
        ApplicationGrantActionV1, ApplicationGrantIntentV1, ApplicationGrantReceiptV1,
        ApplicationGrantSelectorV1, SignedApplicationGrantV1,
    },
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        amount_fee_v3: None,
        quote_ttl_ms: 15_000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    }
}

fn budget() -> PlanBudget {
    PlanBudget {
        transactions: 4,
        transaction_bytes: 8192,
        body_bytes: 32 * 1024,
        access_keys: 128,
    }
}

fn capture_budget() -> CaptureBudget {
    CaptureBudget {
        keys: 128,
        nodes: 4096,
        bytes: 4 * 1024 * 1024,
    }
}

fn context(root: NodeHash) -> Result<BatchContext> {
    Ok(BatchContext {
        chain_id: 7,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [6; 32],
        business_program: application_program_id_v1(),
        semantic_version: 4,
        effect_contract: application_effect_contract_v1(&policy())?,
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: root,
        parent_receipt_root: empty_root(),
        parent_state_version: 0,
        receipt_codec: application_receipt_codec_v1(),
        height: 1,
        slot: 1,
        timestamp_unix_ms: 1,
    })
}

struct Fixture {
    library: PathBuf,
    facade: AoemExecFacade,
    verifier: ApplicationGrantVerifierV1,
    key: MldsaKeyPair,
    identity: UcaAccount,
    initial: StagedStateUpdate,
    memory: Memory,
}

impl Fixture {
    fn new() -> Result<Self> {
        let library = PathBuf::from(
            std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
                .context("explicit trusted NOVOVM_AOEM_TEST_LIBRARY required")?,
        )
        .canonicalize()?;
        let facade = AoemExecFacade::open(&library, AoemExecOpenOptions::default())?;
        let key = facade
            .mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?
            .generate_keypair()?;
        let verifier = facade.application_grant_verifier_v1()?;
        let identity = UcaAccount {
            uca_id: "uca:alice".into(),
            primary_key_ref: derive_primary_key_ref_from_binding_v1(
                UcaKeyAlgo::Mldsa65,
                &key.public_key,
            ),
            primary_key_binding: Some(UcaPrimaryKeyBinding {
                key_algo: UcaKeyAlgo::Mldsa65,
                public_key: key.public_key.clone(),
                proof_type: UcaKeyProofType::GenesisInitializationV1,
                proof_payload: vec![5; 32],
                verified_at: 0,
            }),
            status: UcaStatus::Active,
            created_at: 0,
            updated_at: 0,
        };
        let initial = stage_state_update(
            &Memory::default(),
            empty_root(),
            &identity_record_changes(7, &[1; 32], &identity)?,
        )?;
        let memory = Memory(initial.nodes().clone());
        Ok(Self {
            library,
            facade,
            verifier,
            key,
            identity,
            initial,
            memory,
        })
    }

    fn intent(&self, application: u8) -> Result<ApplicationGrantIntentV1> {
        Ok(ApplicationGrantIntentV1 {
            selector: ApplicationGrantSelectorV1 {
                chain_id: 7,
                genesis: [1; 32],
                identity: UcaIdentityIdV1::new("uca:alice").map_err(anyhow::Error::msg)?,
                application: [application; 32],
                service: [3; 32],
                device: [4; 32],
            },
            issuer_key_ref: self.identity.primary_key_ref.as_slice().try_into()?,
            expected_revision: 0,
            valid_from_height: 1,
            valid_until_height: 20,
            action: ApplicationGrantActionV1::Authorize {
                not_before_height: 1,
                expires_at_height: 100,
            },
        })
    }

    fn sign(&self, intent: ApplicationGrantIntentV1) -> Result<SignedApplicationGrantV1> {
        self.facade.application_grant_signer_v1()?.sign(
            intent,
            &self.key.public_key,
            &self.key.expanded_secret_key,
        )
    }

    fn facts(
        &self,
        signed: &[SignedApplicationGrantV1],
    ) -> Result<Vec<ApplicationGrantSignatureFactV1>> {
        self.verifier
            .verify_batch(signed.to_vec())?
            .into_iter()
            .map(|checked| checked?.into_signature_fact())
            .collect()
    }

    fn plan(
        &self,
        signed: &[SignedApplicationGrantV1],
        context: BatchContext,
    ) -> Result<ApplicationGrantPlanV1> {
        ApplicationGrantBodyV1::prepare(signed.to_vec(), &self.facts(signed)?, &policy(), budget())?
            .bind(context)
    }
}

fn incremental_capture(
    plan: ApplicationGrantPlanV1,
    reader: &dyn StateNodeReader,
) -> Result<ApplicationGrantInputV1> {
    let mut capture = plan.begin_capture(capture_budget())?;
    let mut reads = 0;
    for _ in 0..10_000 {
        match capture.advance(3)? {
            CaptureStep::More => {}
            CaptureStep::NeedRead => {
                let request = capture.next_request()?.context("missing capture request")?;
                assert!(!request.is_empty());
                assert_eq!(request.iter().collect::<BTreeSet<_>>().len(), request.len());
                assert_eq!(capture.next_request()?, Some(request.clone()));
                let values = request
                    .iter()
                    .map(|hash| reader.read_node(hash))
                    .collect::<Result<Vec<_>>>()?;
                capture.accept(values)?;
                reads += 1;
            }
            CaptureStep::Complete => {
                assert!(
                    reads > 0,
                    "the initialized parent must actually be captured"
                );
                return capture.finish();
            }
        }
    }
    anyhow::bail!("incremental component capture did not finish")
}

fn observe(
    reader: &dyn StateNodeReader,
    root: NodeHash,
    height: u64,
    access: &ApplicationGrantAccessV1,
) -> Result<ApplicationGrantObservationV1> {
    let input = OwnedStateInput::capture(reader, root, access.declarations(), capture_budget())?;
    access.read(&input, root, height)
}

fn assert_execution(
    fixture: &Fixture,
    memory: &Memory,
    context: BatchContext,
    signed: &[SignedApplicationGrantV1],
    executed: &ExecutedApplicationGrantBatchV1,
) -> Result<()> {
    assert_eq!(executed.effects().context(), &context);
    assert_eq!(executed.receipts().len(), signed.len());
    let mut changes = Vec::new();
    for (signed, receipt) in signed.iter().zip(executed.receipts()) {
        let access = ApplicationGrantAccessV1::new(signed.intent.selector.clone())?;
        let state = observe(
            memory,
            context.parent_state_root,
            context.parent_height,
            &access,
        )?;
        let checked = fixture.verifier.verify(signed.clone())?;
        changes.extend(access.transition(
            &state,
            &checked,
            context.parent_state_root,
            context.height,
        )?);
        let mut command_id = Sha256::new();
        command_id.update(b"novovm/uca/application-command/v1\0");
        command_id.update(signed.intent.signing_bytes()?);
        assert_eq!(
            receipt,
            &ApplicationGrantReceiptV1 {
                command_id: command_id.finalize().into(),
                grant_id: signed.intent.selector.grant_id()?,
                issuer_key_ref: signed.intent.issuer_key_ref,
                previous_revision: signed.intent.expected_revision,
                revision: signed.intent.expected_revision + 1,
                revoked: matches!(signed.intent.action, ApplicationGrantActionV1::Revoke),
                not_before_height: 1,
                expires_at_height: 100,
            }
        );
        assert_eq!(
            ApplicationGrantReceiptV1::decode(&receipt.encode()?)?,
            *receipt
        );
    }
    let expected = stage_state_update(memory, context.parent_state_root, &changes)?;
    assert_eq!(
        executed.effects().update().parent_root(),
        context.parent_state_root
    );
    assert_eq!(executed.effects().update().root(), expected.root());
    assert_eq!(executed.effects().update().nodes(), expected.nodes());
    let receipts = executed
        .receipts()
        .iter()
        .map(ApplicationGrantReceiptV1::encode)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert_eq!(
        executed.receipt_batch_commitment(),
        ordered_receipt_commitment(context.receipt_codec, &receipts)
    );
    assert_eq!(
        executed.statement_commitment(),
        executed_statement_commitment(
            executed.effects().plan_commitment(),
            expected.root(),
            executed.receipt_batch_commitment(),
        )
    );
    Ok(())
}

fn assert_same_execution(a: &ExecutedApplicationGrantBatchV1, b: &ExecutedApplicationGrantBatchV1) {
    assert_eq!(a.effects().plan_commitment(), b.effects().plan_commitment());
    assert_eq!(a.effects().update().root(), b.effects().update().root());
    assert_eq!(a.effects().update().nodes(), b.effects().update().nodes());
    assert_eq!(a.receipts(), b.receipts());
    assert_eq!(a.receipt_batch_commitment(), b.receipt_batch_commitment());
    assert_eq!(a.statement_commitment(), b.statement_commitment());
}

struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        // The Windows provider appends forward-slash paths, which are invalid
        // under the verbatim prefix returned by canonicalize.
        let root = std::env::temp_dir();
        anyhow::ensure!(
            root.is_absolute(),
            "test temporary directory must be absolute"
        );
        let path = root.join(format!(
            "novovm-application-batch-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        // Only the directory exclusively created by this component fixture.
        if self.0.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .starts_with("novovm-application-batch-")
        }) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn assert_cold_recovery(
    fixture: &Fixture,
    signed: &[SignedApplicationGrantV1],
    executed: ExecutedApplicationGrantBatchV1,
    revoked: ExecutedApplicationGrantBatchV1,
) -> Result<()> {
    let expected_context = *executed.effects().context();
    let expected_root = executed.effects().update().root();
    let expected_receipts = executed.receipts().to_vec();
    let expected_receipt_commitment = executed.receipt_batch_commitment();
    let expected_statement = executed.statement_commitment();
    let expected_body = signed
        .iter()
        .map(SignedApplicationGrantV1::encode)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let packet_budget = PacketBudget::default();
    let packet = PreparedCandidate::from_application_executed(executed, packet_budget)?;
    let directory = TestDirectory::new()?;
    let config = StoreConfig {
        library: fixture.library.clone(),
        database: directory.0.join("candidate.rocksdb"),
        domain: StorageDomain {
            chain_id: expected_context.chain_id,
            genesis_config_commitment: expected_context.genesis_config_commitment,
            protocol_commitment: expected_context.protocol_commitment,
        },
        storage: StorageConfig::default(),
        packet_budget,
    };
    let store = CandidateStore::open(config.clone(), OpenMode::CreateNew)?;
    store.install_unpublished_state(&fixture.initial)?;
    assert!(store.recover(packet.candidate_id())?.is_none());
    let persisted = store.persist(&packet)?;
    assert_eq!(persisted.candidate_id, packet.candidate_id());
    assert_eq!(persisted.state_root, expected_root);
    assert_eq!(persisted.statement_commitment, expected_statement);
    assert!(!persisted.already_present);
    drop(store);

    let reopened = CandidateStore::open(config.clone(), OpenMode::Existing)?;
    let recovered = reopened
        .recover(packet.candidate_id())?
        .context("application candidate missing after RocksDB reopen")?;
    assert!(packet.matches(&recovered));
    assert_eq!(recovered.context(), &expected_context);
    assert_eq!(recovered.parent_state_root(), fixture.initial.root());
    assert_eq!(recovered.state_root(), expected_root);
    assert_eq!(
        recovered.receipt_batch_commitment(),
        expected_receipt_commitment
    );
    assert_eq!(recovered.statement_commitment(), expected_statement);
    assert_eq!(recovered.raw_transactions(), expected_body);
    assert_eq!(recovered.receipt_bytes().len(), expected_receipts.len());
    assert!(recovered.spend_outputs().is_none());
    for (index, receipt) in expected_receipts.iter().enumerate() {
        assert_eq!(&recovered.application_receipt(index)?, receipt);
        assert_eq!(recovered.receipt_bytes()[index], receipt.encode()?);
        let access = ApplicationGrantAccessV1::new(signed[index].intent.selector.clone())?;
        observe(&reopened, expected_root, 1, &access)?.check_at_root(expected_root, 1, 1)?;
    }
    assert!(recovered
        .application_receipt(expected_receipts.len())
        .is_err());
    assert_eq!(recovered.declared_access().len(), 37);

    let revoked_context = *revoked.effects().context();
    let revoked_root = revoked.effects().update().root();
    let revoked_receipts = revoked.receipts().to_vec();
    let revoked_body = revoked.effects().plan().canonical_transactions_for_test();
    assert_eq!(revoked_context.parent_state_root, expected_root);
    assert_eq!(
        (revoked_context.parent_height, revoked_context.height),
        (1, 2)
    );
    assert_eq!(revoked_receipts.len(), 1);
    let revoked_packet = PreparedCandidate::from_application_executed(revoked, packet_budget)?;
    let persisted = reopened.persist(&revoked_packet)?;
    assert_eq!(persisted.state_root, revoked_root);
    assert!(!persisted.already_present);
    drop(reopened);

    let reopened = CandidateStore::open(config, OpenMode::Existing)?;
    let recovered = reopened
        .recover(revoked_packet.candidate_id())?
        .context("revoked application candidate missing after second RocksDB reopen")?;
    assert!(revoked_packet.matches(&recovered));
    assert_eq!(recovered.context(), &revoked_context);
    assert_eq!(recovered.parent_state_root(), expected_root);
    assert_eq!(recovered.state_root(), revoked_root);
    assert_eq!(recovered.raw_transactions(), revoked_body);
    assert_eq!(recovered.receipt_bytes().len(), 1);
    assert_eq!(recovered.application_receipt(0)?, revoked_receipts[0]);
    assert_eq!(recovered.receipt_bytes()[0], revoked_receipts[0].encode()?);
    assert_eq!(
        (
            revoked_receipts[0].previous_revision,
            revoked_receipts[0].revision,
            revoked_receipts[0].revoked,
        ),
        (1, 2, true)
    );
    let access = ApplicationGrantAccessV1::new(signed[0].intent.selector.clone())?;
    let tombstone = observe(&reopened, revoked_root, 2, &access)?;
    let ApplicationGrantStateV1::Present(view) = tombstone.query_state(2)? else {
        anyhow::bail!("cold-recovered revocation must remain queryable")
    };
    assert_eq!(
        (view.state_root, view.height, view.revision, view.revoked),
        (revoked_root, 2, 2, true)
    );
    assert!(tombstone.check_at_root(revoked_root, 2, 2).is_err());
    let other_access = ApplicationGrantAccessV1::new(signed[1].intent.selector.clone())?;
    observe(&reopened, revoked_root, 2, &other_access)?.check_at_root(revoked_root, 2, 1)?;
    let authorization = reopened
        .recover(packet.candidate_id())?
        .context("authorization parent candidate missing after revocation recovery")?;
    assert!(packet.matches(&authorization));
    drop(reopened);
    Ok(())
}

#[test]
#[ignore = "explicit qualified AOEM/ML-DSA/RocksDB; unpublished component fixture, not service activation/publication/login"]
fn real_application_batch_capture_execute_revoke_atomic_failure_and_cold_recovery() -> Result<()> {
    let fixture = Fixture::new()?;
    let root = fixture.initial.root();
    let context = context(root)?;
    let signed = vec![
        fixture.sign(fixture.intent(2)?)?,
        fixture.sign(fixture.intent(9)?)?,
    ];
    let access = ApplicationGrantAccessV1::new(signed[0].intent.selector.clone())?;
    let other_access = ApplicationGrantAccessV1::new(signed[1].intent.selector.clone())?;
    let plan = fixture.plan(&signed, context)?;
    let plan_commitment = plan.plan().commitment();
    let declarations = plan.plan().declared_access();
    assert_eq!(
        declarations.len(),
        35 + 2,
        "33 shared identity keys plus four grant keys"
    );
    let identity_keys: BTreeSet<_> = access.declarations()[..33]
        .iter()
        .map(|d| d.key.clone())
        .collect();
    assert_eq!(
        identity_keys,
        other_access.declarations()[..33]
            .iter()
            .map(|d| d.key.clone())
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(declarations.iter().filter(|d| d.may_put).count(), 4);
    for declaration in declarations {
        if identity_keys.contains(&declaration.key) {
            assert!(!declaration.may_put && !declaration.may_delete);
        } else {
            assert!(declaration.may_put);
        }
    }
    let original = fixture.memory.0.clone();
    let mut session = ComputeSession::open(&fixture.library, 4)?;
    let executed = incremental_capture(plan, &fixture.memory)?.execute(&mut session, TIMEOUT)?;
    assert_eq!(executed.effects().plan_commitment(), plan_commitment);
    assert_execution(&fixture, &fixture.memory, context, &signed, &executed)?;
    assert_eq!(
        fixture.memory.0, original,
        "execution must not mutate its source"
    );
    assert!(matches!(
        observe(&fixture.memory, root, 0, &access)?.query_state(0)?,
        ApplicationGrantStateV1::Missing { .. }
    ));

    let good = fixture.sign(fixture.intent(10)?)?;
    let mut missing = fixture.intent(11)?;
    missing.selector.identity = UcaIdentityIdV1::new("uca:missing").map_err(anyhow::Error::msg)?;
    let mixed = vec![good.clone(), fixture.sign(missing)?];
    let rejected = incremental_capture(fixture.plan(&mixed, context)?, &fixture.memory)?
        .execute(&mut session, TIMEOUT);
    assert!(
        rejected.is_err(),
        "one missing identity rejects the entire batch without output"
    );
    assert_eq!(fixture.memory.0, original);
    let good_access = ApplicationGrantAccessV1::new(good.intent.selector)?;
    assert!(matches!(
        observe(&fixture.memory, root, 0, &good_access)?.query_state(0)?,
        ApplicationGrantStateV1::Missing { .. }
    ));
    let repeated = fixture
        .plan(&signed, context)?
        .capture(&fixture.memory, capture_budget())?
        .execute(&mut session, TIMEOUT)?;
    assert_same_execution(&executed, &repeated);
    assert_eq!(
        fixture.memory.0, original,
        "failed callback must leave the session reusable"
    );

    // Merge content into Memory only to construct the next component parent.
    // There is no service activation, packet publication or consensus decision.
    let active_root = executed.effects().update().root();
    let mut memory = fixture.memory.clone();
    memory.0.extend(executed.effects().update().nodes().clone());
    for key in &identity_keys {
        assert_eq!(
            read_state_value(&memory, active_root, key)?,
            read_state_value(&fixture.memory, root, key)?
        );
    }
    for grant in [&access, &other_access] {
        observe(&memory, active_root, 1, grant)?.check_at_root(active_root, 1, 1)?;
    }
    let next = BatchContext {
        parent_block_hash: [7; 32],
        parent_height: 1,
        parent_state_root: active_root,
        parent_state_version: 2,
        height: 2,
        slot: 2,
        timestamp_unix_ms: 2,
        ..context
    };
    let mut revoke = signed[0].intent.clone();
    revoke.expected_revision = 1;
    let stale = vec![fixture.sign(revoke.clone().into_device_replacement([99; 32], 1)?)?];
    assert!(fixture.plan(&stale, next).is_err());
    let next = BatchContext {
        business_program: uca_registration::replacement_program_id(),
        semantic_version: uca_registration::DEVICE_REPLACEMENT_SEMANTIC_VERSION,
        effect_contract: uca_registration::replacement_effect_contract(&policy())?,
        receipt_codec: uca_registration::receipt_codec(),
        ..next
    };
    let before_failed_replacement = memory.0.clone();
    assert!(incremental_capture(fixture.plan(&stale, next)?, &memory)?
        .execute(&mut session, TIMEOUT)
        .is_err());
    assert_eq!(memory.0, before_failed_replacement);
    observe(&memory, active_root, 1, &access)?.check_at_root(active_root, 1, 1)?;
    let command = revoke.into_device_replacement([99; 32], 0)?;
    let [_, replacement] = command
        .replacement_effects()
        .context("replacement effects")?;
    let replacement_access = ApplicationGrantAccessV1::new(replacement.selector.clone())?;
    let revoke = vec![fixture.sign(command)?];
    let before_revoke = memory.0.clone();
    let revoked = incremental_capture(fixture.plan(&revoke, next)?, &memory)?
        .execute(&mut session, TIMEOUT)?;
    assert_eq!(revoked.receipts().len(), 1);
    assert_eq!(memory.0, before_revoke);
    let revoked_root = revoked.effects().update().root();
    memory.0.extend(revoked.effects().update().nodes().clone());
    let tombstone = observe(&memory, revoked_root, 2, &access)?;
    let ApplicationGrantStateV1::Present(view) = tombstone.query_state(2)? else {
        anyhow::bail!("revocation must remain queryable as a tombstone")
    };
    assert_eq!(
        (view.state_root, view.height, view.revision, view.revoked),
        (revoked_root, 2, 2, true)
    );
    assert_eq!(view.identity_status, IdentityStatusV1::Active);
    assert_eq!(view.issuer_key_ref, signed[0].intent.issuer_key_ref);
    assert!(tombstone.check_at_root(revoked_root, 2, 2).is_err());
    observe(&memory, revoked_root, 2, &replacement_access)?.check_at_root(revoked_root, 2, 1)?;
    observe(&memory, revoked_root, 2, &other_access)?.check_at_root(revoked_root, 2, 1)?;
    let replay_context = BatchContext {
        parent_block_hash: [8; 32],
        parent_height: 2,
        parent_state_root: revoked_root,
        parent_state_version: 3,
        height: 3,
        slot: 3,
        timestamp_unix_ms: 3,
        ..next
    };
    let before_failures = memory.0.clone();
    for replay in [&signed[..1], revoke.as_slice()] {
        assert!(fixture
            .plan(replay, replay_context)?
            .capture(&memory, capture_budget())?
            .execute(&mut session, TIMEOUT)
            .is_err());
    }
    assert_eq!(memory.0, before_failures);

    for grant_expired in [false, true] {
        let mut expired = signed[1].intent.clone();
        expired.expected_revision = 1;
        if grant_expired {
            expired.action = ApplicationGrantActionV1::Authorize {
                not_before_height: 1,
                expires_at_height: 2,
            };
        } else {
            expired.valid_until_height = 2;
        }
        let expired = vec![fixture.sign(expired)?];
        assert!(fixture
            .plan(&expired, next)?
            .capture(&memory, capture_budget())?
            .execute(&mut session, TIMEOUT)
            .is_err());
    }
    for status in [
        UcaStatus::Suspended,
        UcaStatus::Recovering,
        UcaStatus::Revoked,
    ] {
        let mut identity = fixture.identity.clone();
        identity.status = status;
        let update = stage_state_update(
            &fixture.memory,
            root,
            &identity_record_changes(7, &[1; 32], &identity)?,
        )?;
        let mut inactive = fixture.memory.clone();
        inactive.0.extend(update.nodes().clone());
        let inactive_context = BatchContext {
            parent_state_root: update.root(),
            ..context
        };
        let before = inactive.0.clone();
        assert!(fixture
            .plan(&signed, inactive_context)?
            .capture(&inactive, capture_budget())?
            .execute(&mut session, TIMEOUT)
            .is_err());
        assert_eq!(inactive.0, before);
    }
    let other_key = fixture
        .facade
        .mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?
        .generate_keypair()?;
    let mut impostor = fixture.intent(12)?;
    impostor.issuer_key_ref =
        derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &other_key.public_key)
            .as_slice()
            .try_into()?;
    let impostor = vec![fixture.facade.application_grant_signer_v1()?.sign(
        impostor,
        &other_key.public_key,
        &other_key.expanded_secret_key,
    )?];
    assert!(
        fixture
            .plan(&impostor, context)?
            .capture(&fixture.memory, capture_budget())?
            .execute(&mut session, TIMEOUT)
            .is_err(),
        "a valid foreign signature is not alice's identity authority"
    );
    let final_good = incremental_capture(fixture.plan(&signed, context)?, &fixture.memory)?
        .execute(&mut session, TIMEOUT)?;
    assert_same_execution(&executed, &final_good);
    assert_eq!(fixture.memory.0, original);
    assert_cold_recovery(&fixture, &signed, executed, revoked)?;
    Ok(())
}

#[test]
#[ignore = "explicit qualified AOEM and real temporary ML-DSA keys; preparation/binding only, not publication/login"]
fn real_application_body_rejects_duplicate_fact_signature_domain_pins_and_budgets() -> Result<()> {
    let fixture = Fixture::new()?;
    let signed = vec![
        fixture.sign(fixture.intent(2)?)?,
        fixture.sign(fixture.intent(9)?)?,
    ];
    let facts = fixture.facts(&signed)?;
    let policy = policy();
    let prepare = |signed, facts: &[ApplicationGrantSignatureFactV1], budget| {
        ApplicationGrantBodyV1::prepare(signed, facts, &policy, budget)
    };
    assert!(prepare(Vec::new(), &[], budget()).is_err());
    assert!(prepare(signed.clone(), &facts[..1], budget()).is_err());
    let mut swapped = facts.clone();
    swapped.swap(0, 1);
    assert!(prepare(signed.clone(), &swapped, budget()).is_err());
    let mut duplicate = signed[0].intent.clone();
    duplicate.valid_until_height -= 1;
    let duplicate = vec![signed[0].clone(), fixture.sign(duplicate)?];
    assert_ne!(duplicate[0].encode()?, duplicate[1].encode()?);
    assert!(
        prepare(duplicate.clone(), &fixture.facts(&duplicate)?, budget()).is_err(),
        "same selector is forbidden even with distinct verified command bytes"
    );
    for case in 0..4 {
        let mut tampered = signed.clone();
        match case {
            0 => tampered[0].signature[0] ^= 1,
            1 => tampered[0].public_key[0] ^= 1,
            2 => tampered[0].intent.selector.application[0] ^= 1,
            3 => tampered[0].intent.issuer_key_ref[0] ^= 1,
            _ => unreachable!(),
        }
        assert!(fixture.verifier.verify(tampered[0].clone()).is_err());
        assert!(
            prepare(tampered, &facts, budget()).is_err(),
            "tamper case {case}"
        );
    }
    let base = context(fixture.initial.root())?;
    let mutations: &[fn(&mut BatchContext)] = &[
        |c| c.chain_id += 1,
        |c| c.genesis_config_commitment[0] ^= 1,
        |c| c.business_program[0] ^= 1,
        |c| c.receipt_codec[0] ^= 1,
        |c| c.effect_contract[0] ^= 1,
        |c| c.semantic_version = 3,
    ];
    for mutate in mutations {
        let mut invalid = base;
        mutate(&mut invalid);
        assert!(prepare(signed.clone(), &facts, budget())?
            .bind(invalid)
            .is_err());
    }
    for wrong_chain in [false, true] {
        let mut cross = signed[0].intent.clone();
        if wrong_chain {
            cross.selector.chain_id += 1;
        } else {
            cross.selector.genesis[0] ^= 1;
        }
        let cross = vec![fixture.sign(cross)?];
        assert!(
            prepare(cross.clone(), &fixture.facts(&cross)?, budget())?
                .bind(base)
                .is_err(),
            "fresh valid signatures cannot bypass the configured domain"
        );
    }
    let lengths = signed
        .iter()
        .map(|s| s.encode().map(|bytes| bytes.len()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let exact = PlanBudget {
        transactions: 2,
        transaction_bytes: *lengths.iter().max().unwrap(),
        body_bytes: lengths.iter().sum(),
        access_keys: 37,
    };
    assert!(prepare(signed.clone(), &facts, exact)?.bind(base).is_ok());
    for tight in [
        PlanBudget {
            transactions: 1,
            ..exact
        },
        PlanBudget {
            transaction_bytes: exact.transaction_bytes - 1,
            ..exact
        },
        PlanBudget {
            body_bytes: exact.body_bytes - 1,
            ..exact
        },
        PlanBudget {
            access_keys: 36,
            ..exact
        },
    ] {
        assert!(prepare(signed.clone(), &facts, tight).is_err());
    }
    assert!(prepare(signed.clone(), &facts, exact)?
        .bind(base)?
        .begin_capture(CaptureBudget {
            keys: 36,
            ..capture_budget()
        })
        .is_err());
    for tight in [
        CaptureBudget {
            nodes: 0,
            ..capture_budget()
        },
        CaptureBudget {
            bytes: 0,
            ..capture_budget()
        },
    ] {
        assert!(prepare(signed.clone(), &facts, exact)?
            .bind(base)?
            .capture(&fixture.memory, tight)
            .is_err());
    }
    Ok(())
}
