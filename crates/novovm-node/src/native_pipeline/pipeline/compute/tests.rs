//! The channel tests establish ownership/backpressure only. Actual signature and
//! business execution use the explicitly selected AOEM library in the last test.

use super::*;
use crate::native_pipeline::business::direct_nov_fee::FeeState;
use crate::native_pipeline::business::nov_transfer_batch::{
    balance_key, effect_contract, fee_record_changes, program_id, receipt_codec, SEMANTIC_VERSION,
};
use crate::native_pipeline::business::quoted_transfer::Account;
use crate::native_pipeline::ingress::wire::{
    encode_transfer_v3, signing_message, FeePolicy, TransferV3,
};
use crate::native_pipeline::state::frontier::{CaptureBudget, CaptureStep};
use crate::native_pipeline::state::tree::{
    empty_root, stage_state_update, NodeHash, StateChange, StateNodeReader,
};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

mod uca_reuse;

pub(crate) fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
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

pub(crate) fn domain() -> StorageDomain {
    StorageDomain {
        chain_id: 717,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
    }
}

pub(crate) fn context(root: NodeHash) -> BatchContext {
    BatchContext {
        chain_id: domain().chain_id,
        genesis_config_commitment: domain().genesis_config_commitment,
        protocol_commitment: domain().protocol_commitment,
        business_program: program_id(),
        semantic_version: SEMANTIC_VERSION,
        effect_contract: effect_contract(&policy()).unwrap(),
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: root,
        parent_receipt_root: [3; 32],
        parent_state_version: 0,
        receipt_codec: receipt_codec(),
        height: 1,
        slot: 1,
        timestamp_unix_ms: 172_800_123,
    }
}

fn unverified_request(marker: u8) -> PrepareRequest {
    PrepareRequest {
        raw_transactions: vec![vec![marker; 64]].into(),
        context: context(empty_root()),
        policy: policy(),
    }
}

pub(crate) fn control_test_request(root: NodeHash) -> PrepareRequest {
    let mut request = unverified_request(7);
    request.context.parent_state_root = root;
    request
}

#[test]
fn full_channel_returns_the_original_allocation_without_running_a_session() {
    let (sender, receiver) = lanes::single_channel(1, Arc::new(AtomicBool::new(false)));
    let owner = ComputeOwner {
        sender,
        worker: thread::spawn(|| {}),
        integer: None,
    };
    let first = match owner.try_prepare(unverified_request(1)).unwrap() {
        Submission::Accepted(ticket) => ticket,
        Submission::Backpressured(_) => panic!("empty channel refused request"),
    };
    let request = unverified_request(2);
    let pointer = request.raw_transactions[0].as_ptr();
    let retained = match owner.try_prepare(request).unwrap() {
        Submission::Backpressured(retained) => retained,
        Submission::Accepted(_) => panic!("full channel accepted second request"),
    };
    assert_eq!(retained.raw_transactions[0].as_ptr(), pointer);
    assert_eq!(retained.raw_transactions, vec![vec![2; 64]]);
    drop(first); // Notification loss does not remove the accepted command.
    let Command::Prepare { request, reply, .. } = receiver.try_recv().unwrap().into_command()
    else {
        panic!("wrong accepted command")
    };
    assert_eq!(request.raw_transactions, vec![vec![1; 64]]);
    assert!(reply
        .send(Err(anyhow::anyhow!("admission-only fixture")))
        .is_err());
    drop(receiver);
    assert!(owner.try_prepare(retained).is_err());
    owner.shutdown().unwrap();
}

#[test]
fn tickets_are_nonblocking_and_consumed_exactly_once_even_on_error() {
    let (sender, receiver) = mpsc::channel::<Result<u32>>();
    let mut ticket = ComputeTicket {
        receiver,
        consumed: false,
    };
    assert_eq!(ticket.try_take().unwrap(), None);
    sender.send(Ok(19)).unwrap();
    assert_eq!(ticket.try_take().unwrap(), Some(19));
    assert!(ticket.try_take().is_err());

    let (sender, receiver) = mpsc::channel::<Result<u32>>();
    let mut ticket = ComputeTicket {
        receiver,
        consumed: false,
    };
    sender
        .send(Err(anyhow::anyhow!("ordinary rejection")))
        .unwrap();
    assert!(ticket
        .try_take()
        .unwrap_err()
        .to_string()
        .contains("ordinary rejection"));
    assert!(ticket.try_take().is_err());
    let (sender, receiver) = mpsc::channel::<Result<u32>>();
    let mut ticket = ComputeTicket {
        receiver,
        consumed: false,
    };
    drop(sender);
    assert!(ticket
        .try_take()
        .unwrap_err()
        .to_string()
        .contains("disconnected"));
    assert!(ticket.try_take().is_err());
}

#[test]
fn ordinary_rejection_does_not_latch_but_unwind_rejects_all_later_work() {
    let mut panicked = false;
    assert!(run_checked::<()>(&mut panicked, || anyhow::bail!("bad external input")).is_err());
    assert!(!panicked);
    assert_eq!(run_checked(&mut panicked, || Ok(23)).unwrap(), 23);
    assert!(run_checked::<()>(&mut panicked, || panic!("intentional owner unwind")).is_err());
    assert!(panicked);
    let ran = AtomicBool::new(false);
    assert!(run_checked(&mut panicked, || {
        ran.store(true, Ordering::SeqCst);
        Ok(())
    })
    .is_err());
    assert!(!ran.load(Ordering::SeqCst));
    struct ExplosiveDrop;
    impl Drop for ExplosiveDrop {
        fn drop(&mut self) {
            panic!("panic payload destructor must not run");
        }
    }
    let mut panicked = false;
    assert!(run_checked::<()>(&mut panicked, || std::panic::panic_any(ExplosiveDrop)).is_err());
    assert!(panicked);
}

#[test]
fn configured_domain_cannot_be_replaced_by_request_or_capture() {
    let original = context(empty_root());
    domain_matches(domain(), &original).unwrap();
    let mut changed = original;
    changed.chain_id += 1;
    assert!(domain_matches(domain(), &changed).is_err());
    let mut changed = original;
    changed.genesis_config_commitment[0] ^= 1;
    assert!(domain_matches(domain(), &changed).is_err());
    let mut changed = original;
    changed.protocol_commitment[0] ^= 1;
    assert!(domain_matches(domain(), &changed).is_err());
}

#[test]
fn all_cross_owner_payloads_are_owned_send_types() {
    fn send<T: Send>() {}
    send::<PrepareRequest>();
    send::<NovTransferPlan>();
    send::<NovCapturedInput>();
    send::<ComputedCandidate>();
}

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

pub(crate) fn account(seed: u8) -> Account {
    let key = SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes();
    Account::try_from(Sha256::digest(key)[12..].to_vec()).unwrap()
}

pub(crate) fn signed(seed: u8, amount: u128) -> Vec<u8> {
    let signer = SigningKey::from_bytes(&[seed; 32]);
    let mut tx = TransferV3 {
        chain_id: domain().chain_id,
        from: account(seed).as_bytes().to_vec(),
        to: account(9).as_bytes().to_vec(),
        asset: "NOV".into(),
        amount,
        nonce: 0,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 0,
            slippage_bps: 0,
        },
        signature: Vec::new(),
    };
    let signature = signer.sign(&signing_message(&tx).unwrap());
    tx.signature = signer.verifying_key().to_bytes().to_vec();
    tx.signature.extend_from_slice(&signature.to_bytes());
    encode_transfer_v3(&tx).unwrap()
}

fn take_admitted<R, T>(
    mut request: R,
    mut submit: impl FnMut(R) -> Result<Submission<R, T>>,
) -> Result<ComputeTicket<T>> {
    let start = Instant::now();
    loop {
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "compute admission deadline"
        );
        match submit(request)? {
            Submission::Accepted(ticket) => return Ok(ticket),
            Submission::Backpressured(retained) => request = retained,
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn bounded_take<T>(ticket: &mut ComputeTicket<T>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = ticket.try_take()? {
            return Ok(result);
        }
        ensure!(
            Instant::now() < deadline,
            "compute ticket did not progress independently"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
#[ignore = "requires real AOEM/Vulkan; held integer lane, real concurrent authentication and drain"]
fn real_integer_lane_does_not_block_authentication_and_drains_accepted_work() -> Result<()> {
    let cfg = crate::native_pipeline::pipeline::tests::config(PathBuf::from("unused-storage"));
    let config = ComputeConfig {
        library: std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted AOEM required")?
            .into(),
        workers: cfg.workers,
        execution_backend: super::super::ExecutionBackend::AoemIntegerV7,
        signature_backend: Default::default(),
        queue_capacity: 2,
        authentication: cfg.authentication,
        plan: cfg.plan,
        packet: cfg.store.packet_budget,
        capture: cfg.capture,
        timeout: cfg.compute_timeout,
        domain: cfg.store.domain,
        signatures: Arc::new(super::super::SignatureWork::default()),
    };
    let owner = ComputeOwner::start(config, Some(thread::current()))?;
    let mut changes = fee_record_changes(&policy(), &FeeState::default())?;
    changes.push(StateChange::Put {
        key: balance_key(&account(1)),
        value: 10_000u128.to_le_bytes().to_vec(),
    });
    let update = stage_state_update(&Memory::default(), empty_root(), &changes)?;
    let memory = Memory(update.nodes().clone());
    let raw = signed(1, 100);
    let request = || PrepareRequest {
        raw_transactions: vec![raw.clone()].into(),
        context: context(update.root()),
        policy: policy(),
    };
    let first = capture(
        take_admitted(request(), |r| owner.try_prepare(r))?.wait()?,
        &memory,
    )?;
    let second = capture(
        take_admitted(request(), |r| owner.try_prepare(r))?.wait()?,
        &memory,
    )?;
    drop(memory);
    let (entered, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    ensure!(
        owner
            .integer
            .as_ref()
            .unwrap()
            .sender
            .try_send(Command::Hold {
                entered,
                release: held
            })
            .is_ok(),
        "hold admission failed"
    );
    observed.recv_timeout(Duration::from_secs(5))?;
    let mut execution = take_admitted(first, |r| owner.try_execute(r))?;
    let mut auth = take_admitted(vec![AdmissionInput::Raw(raw.clone())], |r| {
        owner.try_admit_signatures(r)
    })?;
    let admitted = bounded_take(&mut auth)?;
    assert_eq!(admitted.rows.len(), 1);
    assert!(admitted.rows[0].result.is_ok());
    assert!(admitted.peak_callbacks > 0); // Actual AOEM worker, not fabricated ACK.
    let mut bad = raw;
    *bad.last_mut().unwrap() ^= 1;
    let mut rejected = take_admitted(vec![AdmissionInput::Raw(bad)], |r| {
        owner.try_admit_signatures(r)
    })?;
    assert!(bounded_take(&mut rejected)?.rows[0].result.is_err());
    assert!(
        execution.try_take()?.is_none(),
        "held integer lane unexpectedly ran"
    );
    release.send(())?;
    let first = bounded_take(&mut execution)?;
    let second = take_admitted(second, |r| owner.try_execute(r))?;
    owner.shutdown()?; // Accepted work is drained, even before taking its reply.
    let second = second.wait()?;
    assert_eq!(first.packet.records(), second.packet.records());
    assert_eq!(first.packet.state_root(), second.packet.state_root());
    assert_eq!(
        first.packet.statement_commitment(),
        second.packet.statement_commitment()
    );
    Ok(())
}

fn capture(plan: NovTransferPlan, memory: &Memory) -> Result<NovCapturedInput> {
    let mut capture = plan.begin_capture(CaptureBudget {
        keys: 4096,
        nodes: 4096,
        bytes: 1024 * 1024,
    })?;
    let mut rounds = 0usize;
    loop {
        rounds += 1;
        ensure!(rounds < 10_000, "bounded test capture did not finish");
        match capture.advance(256)? {
            CaptureStep::More => {}
            CaptureStep::NeedRead => {
                let hashes = capture
                    .next_request()?
                    .context("missing frontier request")?;
                ensure!(
                    !hashes.is_empty() && hashes.len() <= 64,
                    "invalid frontier batch"
                );
                capture.accept(
                    hashes
                        .iter()
                        .map(|hash| memory.0.get(hash).cloned())
                        .collect(),
                )?;
            }
            CaptureStep::Complete => return capture.finish(),
        }
    }
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; resident owner, not node/TPS evidence"]
fn real_resident_owner_authenticates_and_executes_repeated_owned_captures() -> Result<()> {
    let config = ComputeConfig {
        library: std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted AOEM library required")?
            .into(),
        workers: 2,
        execution_backend: Default::default(),
        signature_backend: Default::default(),
        queue_capacity: 2,
        authentication: AuthenticationBudget {
            transactions: 16,
            transaction_bytes: 4096,
            body_bytes: 65536,
        },
        plan: PlanBudget {
            transactions: 16,
            transaction_bytes: 4096,
            body_bytes: 65536,
            access_keys: 4096,
        },
        packet: PacketBudget::default(),
        capture: CaptureBudget {
            keys: 4096,
            nodes: 4096,
            bytes: 1024 * 1024,
        },
        timeout: Duration::from_secs(30),
        domain: domain(),
        signatures: Arc::new(super::super::SignatureWork::default()),
    };
    let owner = ComputeOwner::start(config, Some(thread::current()))?;
    let mut changes = fee_record_changes(&policy(), &FeeState::default())?;
    for seed in [1, 2] {
        changes.push(StateChange::Put {
            key: balance_key(&account(seed)),
            value: 10_000u128.to_le_bytes().to_vec(),
        });
    }
    let update = stage_state_update(&Memory::default(), empty_root(), &changes)?;
    let memory = Memory(update.nodes().clone());
    let raw_transactions = vec![signed(1, 100), signed(2, 200)];
    let make_request = || PrepareRequest {
        raw_transactions: raw_transactions.clone().into(),
        context: context(update.root()),
        policy: policy(),
    };

    // Invalid external signature must not destroy or recreate the resident session.
    let mut bad = make_request();
    *bad.raw_transactions[0].last_mut().unwrap() ^= 1;
    assert!(take_admitted(bad, |request| owner.try_prepare(request))?
        .wait()
        .is_err());
    let first_plan = take_admitted(make_request(), |request| owner.try_prepare(request))?.wait()?;
    let identity = first_plan.commitment();
    let first = capture(first_plan, &memory)?;
    let second_plan =
        take_admitted(make_request(), |request| owner.try_prepare(request))?.wait()?;
    assert_eq!(second_plan.commitment(), identity);
    let second = capture(second_plan, &memory)?;
    drop(memory); // No live reader/database survives into either AOEM execution.
    let first = take_admitted(first, |input| owner.try_execute(input))?.wait()?;
    let second = take_admitted(second, |input| owner.try_execute(input))?.wait()?;
    assert_eq!(first.observation.components, 2);
    assert_eq!(first.observation.credit_only_accounts, 1);
    assert_eq!(first.observation.recomputed_transactions, 0);
    assert!(first.observation.peak_callbacks > 0);
    assert_eq!(first.packet.candidate_id(), identity);
    assert_eq!(first.packet.state_root(), second.packet.state_root());
    assert_eq!(
        first.packet.statement_commitment(),
        second.packet.statement_commitment()
    );
    assert_eq!(first.packet.records(), second.packet.records());
    owner.shutdown()?;
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_OLD_TEST_LIBRARY without v7; real startup fail-closed"]
fn real_compute_owner_gpu_selection_rejects_old_library_without_cpu_fallback() -> Result<()> {
    let cfg = crate::native_pipeline::pipeline::tests::config(PathBuf::from("unused-storage"));
    let config = ComputeConfig {
        library: std::env::var_os("NOVOVM_AOEM_OLD_TEST_LIBRARY")
            .context("explicit old AOEM library required")?
            .into(),
        workers: cfg.workers,
        execution_backend: crate::native_pipeline::pipeline::ExecutionBackend::AoemIntegerV7,
        signature_backend: Default::default(),
        queue_capacity: cfg.max_batches,
        authentication: cfg.authentication,
        plan: cfg.plan,
        packet: cfg.store.packet_budget,
        capture: cfg.capture,
        timeout: cfg.compute_timeout,
        domain: cfg.store.domain,
        signatures: Arc::new(super::super::SignatureWork::default()),
    };
    let error = match ComputeOwner::start(config, None) {
        Ok(owner) => {
            owner.shutdown()?;
            anyhow::bail!("old library unexpectedly started the explicitly GPU owner");
        }
        Err(error) => error,
    };
    ensure!(
        format!("{error:#}").contains("does not support checked integer resources v7"),
        "wrong reason for startup rejection: {error:#}"
    );
    Ok(())
}
