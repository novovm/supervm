//! Real V3 signatures and detached parent nonce planning on AOEM. Nonce leaves
//! here use a test-only record layout; staging them is NOT admission, charging,
//! a receipt, persistence, business execution, or a finalized transaction.

use anyhow::{ensure, Context, Result};
use ed25519_dalek::{Signer, SigningKey};
use novovm_aoem::{ComputeSession, ComputeTask};
use novovm_host::execution::plan::{BatchContext, PlanBudget, UnpublishedBatchEffects};
use novovm_host::ingress::authentication::check_nonce_sequence;
use novovm_host::ingress::batch::{authenticate_batch, AuthenticationBudget};
use novovm_host::ingress::wire::{encode_transfer_v3, signing_message, FeePolicy, TransferV3};
use novovm_host::state::frontier::{CaptureBudget, DeclaredAccess};
use novovm_host::state::tree::{
    empty_root, read_state_value, stage_state_update, NodeHash, StateChange, StateNodeReader,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn library() -> PathBuf {
    std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .expect("set explicit trusted NOVOVM_AOEM_TEST_LIBRARY for this component test")
        .into()
}

fn budget() -> AuthenticationBudget {
    AuthenticationBudget {
        transactions: 64,
        transaction_bytes: 1024,
        body_bytes: 64 * 1024,
    }
}

fn signed(seed: u8, nonce: u64, full_account: bool) -> Result<Vec<u8>> {
    let signing_key = SigningKey::from_bytes(&[seed; 32]);
    let public_key = signing_key.verifying_key().to_bytes();
    let mut tx = TransferV3 {
        chain_id: 91,
        from: if full_account {
            public_key.to_vec()
        } else {
            Sha256::digest(public_key)[12..].to_vec()
        },
        to: vec![77; 20],
        asset: "NOV".to_owned(),
        amount: 17,
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".to_owned(),
            max_pay_amount: 100,
            slippage_bps: 50,
        },
        signature: Vec::new(),
    };
    let signature = signing_key.sign(&signing_message(&tx)?);
    tx.signature.extend_from_slice(&public_key);
    tx.signature.extend_from_slice(&signature.to_bytes());
    encode_transfer_v3(&tx)
}

#[derive(Clone, Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

struct Source {
    data: Memory,
    reads: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl StateNodeReader for Source {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.data.read_node(hash)
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn nonce_key(identity: [u8; 32]) -> Vec<u8> {
    [b"fixture/nonce/".as_slice(), &identity].concat()
}

fn context(root: NodeHash) -> BatchContext {
    BatchContext {
        chain_id: 91,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: [3; 32],
        semantic_version: 1,
        effect_contract: [4; 32],
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: root,
        parent_receipt_root: empty_root(),
        parent_state_version: 0,
        receipt_codec: [5; 32],
        height: 1,
        slot: 0,
        timestamp_unix_ms: 1,
    }
}

fn plan_budget() -> PlanBudget {
    PlanBudget {
        transactions: 64,
        transaction_bytes: 1024,
        body_bytes: 64 * 1024,
        access_keys: 32,
    }
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; signed component, not chain acceptance"]
fn real_signed_batch_keeps_body_identity_and_detached_parent_nonce_inputs() -> Result<()> {
    let mut session = ComputeSession::open(&library(), 4)?;
    let mut raw = Vec::new();
    for nonce in 0..2 {
        for seed in 1..=32 {
            raw.push(signed(seed, nonce, nonce == 1)?);
        }
    }
    let reference_raw = raw.clone();
    let batch = authenticate_batch(&mut session, 91, raw, budget(), Duration::from_secs(10))?;
    assert_eq!(batch.transactions().len(), 64);
    eprintln!(
        "64 genuine V3 signatures checked on AOEM; observed callback peak={}",
        batch.peak_callbacks()
    );
    let mut nonces = BTreeMap::new();
    for (index, tx) in batch.transactions().iter().enumerate() {
        nonces.insert(tx.nonce_identity(), 0);
        if index >= 32 {
            assert_eq!(
                tx.nonce_identity(),
                batch.transactions()[index - 32].nonce_identity()
            );
            assert_ne!(
                tx.transfer().from,
                batch.transactions()[index - 32].transfer().from
            );
        }
    }
    assert_eq!(nonces.len(), 32);
    let parent = stage_state_update(
        &Memory::default(),
        empty_root(),
        &nonces
            .keys()
            .map(|identity| StateChange::Put {
                key: nonce_key(*identity),
                value: 0u64.to_be_bytes().to_vec(),
            })
            .collect::<Vec<_>>(),
    )?;
    let reference = Memory(parent.nodes().clone());
    let reads = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let source = Source {
        data: reference.clone(),
        reads: reads.clone(),
        drops: drops.clone(),
    };
    let plan = batch.bind(
        context(parent.root()),
        nonces
            .keys()
            .map(|identity| DeclaredAccess {
                key: nonce_key(*identity),
                may_put: true,
                may_delete: false,
            })
            .collect(),
        plan_budget(),
    )?;
    assert_eq!(plan.plan().raw_transactions(), reference_raw);
    let commitment = plan.plan().commitment();
    let input = plan.capture(
        &source,
        CaptureBudget {
            keys: 32,
            nodes: 128,
            bytes: 65536,
        },
    )?;
    let captured_reads = reads.load(Ordering::SeqCst);
    ensure!(captured_reads > 0);
    drop(source);
    assert_eq!(drops.load(Ordering::SeqCst), 1);

    let result: Arc<Mutex<Option<UnpublishedBatchEffects>>> = Arc::new(Mutex::new(None));
    let output = result.clone();
    let caller = std::thread::current().id();
    let task: ComputeTask = Box::new(move || {
        ensure!(std::thread::current().id() != caller);
        let mut parent_nonces = BTreeMap::new();
        for transaction in input.transactions() {
            let identity = transaction.nonce_identity();
            if let std::collections::btree_map::Entry::Vacant(entry) = parent_nonces.entry(identity)
            {
                let value = input
                    .read(&nonce_key(identity))?
                    .context("missing fixture parent nonce")?;
                entry.insert(u64::from_be_bytes(
                    value
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("bad nonce bytes"))?,
                ));
            }
        }
        let transitions = check_nonce_sequence(input.transactions(), &parent_nonces)?;
        ensure!(transitions.len() == 64);
        let effects = input.stage(
            &transitions
                .into_iter()
                .map(|transition| StateChange::Put {
                    key: nonce_key(transition.identity),
                    value: transition.after.to_be_bytes().to_vec(),
                })
                .collect::<Vec<_>>(),
        )?;
        let root = effects.update().root().to_vec();
        *output.lock().unwrap() = Some(effects);
        Ok(root)
    });
    let report = session.execute(vec![task], Duration::from_secs(10))?;
    let effects = result
        .lock()
        .unwrap()
        .take()
        .context("missing tentative output")?;
    assert_eq!(effects.plan_commitment(), commitment);
    assert_eq!(report.outputs, vec![effects.update().root().to_vec()]);
    assert_eq!(effects.update().parent_root(), parent.root());
    let mut updated = reference.clone();
    updated.0.extend(effects.update().nodes().clone());
    for identity in nonces.keys() {
        assert_eq!(
            read_state_value(&reference, parent.root(), &nonce_key(*identity))?,
            Some(0u64.to_be_bytes().to_vec())
        );
        assert_eq!(
            read_state_value(&updated, effects.update().root(), &nonce_key(*identity))?,
            Some(2u64.to_be_bytes().to_vec())
        );
    }
    assert_eq!(reads.load(Ordering::SeqCst), captured_reads);
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; rejects complete batches, no state admission"]
fn bad_signature_domain_duplicate_and_budget_never_return_an_accepted_batch() -> Result<()> {
    let path = library();
    let valid = signed(1, 0, false)?;
    let mut bad_signature = valid.clone();
    *bad_signature.last_mut().unwrap() ^= 1;
    let mut session = ComputeSession::open(&path, 4)?;
    assert!(authenticate_batch(
        &mut session,
        91,
        vec![valid.clone(), bad_signature],
        budget(),
        Duration::from_secs(10)
    )
    .is_err());
    assert!(authenticate_batch(
        &mut session,
        92,
        vec![valid.clone()],
        budget(),
        Duration::from_secs(10)
    )
    .is_err());
    assert!(authenticate_batch(
        &mut session,
        91,
        vec![valid.clone(), valid.clone()],
        budget(),
        Duration::from_secs(10)
    )
    .is_err());
    for limits in [
        AuthenticationBudget {
            transactions: 0,
            ..budget()
        },
        AuthenticationBudget {
            transaction_bytes: valid.len() - 1,
            ..budget()
        },
        AuthenticationBudget {
            body_bytes: valid.len() - 1,
            ..budget()
        },
    ] {
        assert!(authenticate_batch(
            &mut session,
            91,
            vec![valid.clone()],
            limits,
            Duration::from_secs(10)
        )
        .is_err());
    }
    // Invalid signatures/domains, pre-admission limits and duplicate detection
    // do not poison the native owner; a peer cannot kill it with invalid input.
    let batch = authenticate_batch(
        &mut session,
        91,
        vec![valid],
        budget(),
        Duration::from_secs(10),
    )?;
    let mut wrong_chain = context(empty_root());
    wrong_chain.chain_id = 92;
    assert!(batch
        .bind(
            wrong_chain,
            vec![DeclaredAccess {
                key: b"fixture".to_vec(),
                may_put: true,
                may_delete: false
            }],
            plan_budget()
        )
        .is_err());
    Ok(())
}
