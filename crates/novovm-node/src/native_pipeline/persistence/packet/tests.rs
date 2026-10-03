//! Pure packet codec tests. The private fixture constructor is deliberately not
//! an AOEM execution claim; integration tests construct the public packet only
//! from a real ExecutedNovBatch.

use super::*;
use crate::native_pipeline::business::direct_nov_fee::FeeFailureCode;
use crate::native_pipeline::business::nov_transfer_batch::NovTransferReceipt;
use crate::native_pipeline::business::quoted_transfer::{
    Account, BalanceDelta, TransferDelta, TransferFailure,
};
use crate::native_pipeline::ingress::wire::{
    encode_transfer_v3, signing_message, FeePolicy, TransferV3,
};
use crate::native_pipeline::state::tree::{
    empty_root, stage_state_update, StateChange, StateNodeReader,
};
use ed25519_dalek::{Signer, SigningKey};

struct Empty;
impl StateNodeReader for Empty {
    fn read_node(&self, _: &NodeHash) -> Result<Option<Vec<u8>>> {
        anyhow::bail!("an empty parent must not read live state")
    }
}

struct Fixture {
    plan: BatchPlan,
    root: NodeHash,
    nodes: BTreeMap<NodeHash, Vec<u8>>,
    receipts: Vec<NovTransferReceipt>,
}

impl Fixture {
    fn new(nonces: &[u64], padding: usize, leaves: u32) -> Self {
        let key = SigningKey::from_bytes(&[7; 32]);
        let public = key.verifying_key().to_bytes();
        let mut identity = Sha256::new();
        identity.update(b"novovm-native-auth-nonce-identity-v1");
        identity.update(9u64.to_be_bytes());
        identity.update(b"novovm-native-auth/ed25519-public-key/v2\0");
        identity.update(public);
        let identity: NodeHash = identity.finalize().into();
        let mut raw = Vec::new();
        let mut receipts = Vec::new();
        for (index, nonce) in nonces.iter().copied().enumerate() {
            // The same signer has distinct 20/32-byte balance accounts but one
            // nonce identity. The receipt decoder must preserve that relation.
            let from = if index % 2 == 0 {
                Sha256::digest(public)[12..].to_vec()
            } else {
                public.to_vec()
            };
            let mut tx = TransferV3 {
                chain_id: 9,
                from,
                to: vec![8; 20],
                asset: if padding == 0 {
                    "NOV".into()
                } else {
                    " ".repeat(padding)
                },
                amount: 5,
                nonce,
                fee_policy: FeePolicy {
                    pay_asset: "NOV".into(),
                    max_pay_amount: 1,
                    slippage_bps: 0,
                },
                signature: Vec::new(),
            };
            let signature = key.sign(&signing_message(&tx).unwrap());
            tx.signature = [public.as_slice(), signature.to_bytes().as_slice()].concat();
            let hash = canonical_tx_hash(&tx).unwrap();
            let failure = FeeFailure {
                code: FeeFailureCode::MaxPayExceeded,
                reason: "fixture fee cap".into(),
            };
            receipts.push(NovTransferReceipt {
                tx_hash: hash,
                signer_identity: identity,
                delta: TransferDelta {
                    tx_hash: hash,
                    payer: BalanceDelta {
                        account: Account::try_from(tx.from.clone()).unwrap(),
                        before: 100,
                        after: 100,
                    },
                    recipient: BalanceDelta {
                        account: Account::try_from(tx.to.clone()).unwrap(),
                        before: 0,
                        after: 0,
                    },
                    nonce_identity: identity_hex(&identity),
                    nonce_before: nonce,
                    nonce_after: nonce.checked_add(1).unwrap(),
                    fee_funding_delta: 0,
                },
                failure: Some(TransferFailure::Fee(failure.reason.clone())),
                quote: None,
                fee_failure: Some(failure),
                journal: None,
                clear_clearing_candidates: false,
            });
            raw.push(encode_transfer_v3(&tx).unwrap());
        }
        let changes: Vec<_> = (0..leaves)
            .map(|n| StateChange::Put {
                key: n.to_be_bytes().to_vec(),
                value: vec![3; 32],
            })
            .collect();
        let staged = stage_state_update(&Empty, empty_root(), &changes).unwrap();
        let context = BatchContext {
            chain_id: 9,
            genesis_config_commitment: [1; 32],
            protocol_commitment: [2; 32],
            business_program: program_id(),
            semantic_version: SEMANTIC_VERSION,
            effect_contract: [3; 32],
            parent_block_hash: [0; 32],
            parent_height: 0,
            parent_state_root: empty_root(),
            parent_receipt_root: [4; 32],
            parent_state_version: 0,
            receipt_codec: receipt_codec(),
            height: 1,
            slot: 3,
            timestamp_unix_ms: 1234,
        };
        let plan = BatchPlan::new(
            context,
            raw,
            vec![DeclaredAccess {
                key: b"declared".to_vec(),
                may_put: true,
                may_delete: true,
            }],
            PacketBudget::default().plan(),
        )
        .unwrap();
        Self {
            plan,
            root: staged.root(),
            nodes: staged.nodes().clone(),
            receipts,
        }
    }

    fn packet(&self, budget: PacketBudget) -> Result<PreparedCandidate> {
        let receipts: Vec<_> = self
            .receipts
            .iter()
            .map(|r| postcard::to_allocvec(r).unwrap())
            .collect();
        let receipt = receipt_hash(self.plan.context().receipt_codec, &receipts);
        prepare(
            &self.plan,
            self.root,
            &receipts,
            &self.nodes,
            receipt,
            statement_hash(self.plan.commitment(), self.root, receipt),
            budget,
        )
    }
}

fn load(
    packet: &PreparedCandidate,
    records: &BTreeMap<Vec<u8>, Vec<u8>>,
    budget: PacketBudget,
) -> Result<Option<StoredCandidate>> {
    StoredCandidate::load(packet.candidate_id(), budget, |keys| {
        assert!(keys.len() <= READ_BATCH_KEYS);
        Ok(keys.iter().map(|key| records.get(key).cloned()).collect())
    })
}

fn document(packet: &PreparedCandidate) -> Vec<u8> {
    let marker = Marker::decode(
        &packet.records[&marker_key(packet.candidate_id())],
        packet.candidate_id(),
        PacketBudget::default(),
    )
    .unwrap();
    (0..marker.chunks)
        .flat_map(|index| packet.records[&document_key(packet.candidate_id(), index)].clone())
        .collect()
}

#[test]
fn exact_packet_roundtrip_keeps_body_access_receipts_nodes_and_all_commitments() {
    let fixture = Fixture::new(&[4, 5, 6], 0, 3);
    let packet = fixture.packet(PacketBudget::default()).unwrap();
    let stored = load(&packet, packet.records(), PacketBudget::default())
        .unwrap()
        .unwrap();
    assert!(packet.matches(&stored));
    assert_eq!(stored.record_bytes(), packet.record_bytes());
    assert_eq!(stored.context(), fixture.plan.context());
    assert_eq!(stored.raw_transactions(), fixture.plan.raw_transactions());
    let access = |items: &[DeclaredAccess]| {
        items
            .iter()
            .map(|item| (item.key.clone(), item.may_put, item.may_delete))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        access(stored.declared_access()),
        access(fixture.plan.declared_access())
    );
    assert_eq!(stored.nodes(), &fixture.nodes);
    assert_eq!(stored.state_root(), fixture.root);
    assert_eq!(stored.parent_state_root(), empty_root());
    assert_eq!(stored.plan_commitment(), fixture.plan.commitment());
    assert_eq!(
        stored.receipt_batch_commitment(),
        packet.receipt_batch_commitment()
    );
    assert_eq!(stored.statement_commitment(), packet.statement_commitment());
    assert_eq!(
        stored.receipt_bytes(),
        fixture
            .receipts
            .iter()
            .map(|r| postcard::to_allocvec(r).unwrap())
            .collect::<Vec<_>>()
    );
    let repeated = fixture
        .packet(PacketBudget {
            max_bytes: 64 * 1024 * 1024,
            ..PacketBudget::default()
        })
        .unwrap();
    assert_eq!(packet.records(), repeated.records());
}

#[test]
fn marker_absence_is_not_completion_but_completed_missing_records_never_repair() {
    let packet = Fixture::new(&[0], 0, 2)
        .packet(PacketBudget::default())
        .unwrap();
    let mut records = packet.records.clone();
    records.remove(&marker_key(packet.candidate_id()));
    assert!(load(&packet, &records, PacketBudget::default())
        .unwrap()
        .is_none());
    for key in packet.records.keys().filter(|key| key[0] != b'c') {
        let mut missing = packet.records.clone();
        missing.remove(key);
        let before = missing.clone();
        assert!(load(&packet, &missing, PacketBudget::default()).is_err());
        assert_eq!(missing, before);
    }
}

#[test]
fn altered_marker_document_and_node_or_hidden_tail_are_rejected() {
    let packet = Fixture::new(&[0], 0, 2)
        .packet(PacketBudget::default())
        .unwrap();
    for key in packet.records.keys() {
        let mut records = packet.records.clone();
        records.get_mut(key).unwrap()[0] ^= 1;
        assert!(load(&packet, &records, PacketBudget::default()).is_err());
    }
    let mut records = packet.records.clone();
    records
        .get_mut(&marker_key(packet.candidate_id()))
        .unwrap()
        .push(0);
    assert!(load(&packet, &records, PacketBudget::default()).is_err());
    let header = Marker::decode(
        &packet.records[&marker_key(packet.candidate_id())],
        packet.candidate_id(),
        PacketBudget::default(),
    )
    .unwrap();
    let mut records = packet.records.clone();
    records.insert(document_key(packet.candidate_id(), header.chunks), vec![1]);
    assert!(load(&packet, &records, PacketBudget::default()).is_err());
    assert!(
        StoredCandidate::load([9; 32], PacketBudget::default(), |keys| Ok(keys
            .iter()
            .map(|_| Some(packet.records[&marker_key(packet.candidate_id())].clone()))
            .collect()))
        .is_err()
    );
}

#[test]
fn reads_are_bounded_and_response_count_errors_never_become_absence() {
    let packet = Fixture::new(&[0], CHUNK_BYTES + 33, 80)
        .packet(PacketBudget::default())
        .unwrap();
    let mut lengths = Vec::new();
    let stored = StoredCandidate::load(packet.candidate_id(), PacketBudget::default(), |keys| {
        lengths.push(keys.len());
        Ok(keys
            .iter()
            .map(|key| packet.records.get(key).cloned())
            .collect())
    })
    .unwrap()
    .unwrap();
    assert!(packet.matches(&stored));
    assert!(lengths.contains(&64));
    assert!(lengths.iter().all(|count| *count <= 64));
    assert!(packet.records.keys().filter(|key| key[0] == b'd').count() >= 2);
    for response in [vec![], vec![None, None]] {
        assert!(
            StoredCandidate::load(packet.candidate_id(), PacketBudget::default(), |_| Ok(
                response.clone()
            ))
            .is_err()
        );
    }
    assert!(StoredCandidate::load(
        packet.candidate_id(),
        PacketBudget::default(),
        |_| anyhow::bail!("read failure")
    )
    .is_err());
}

#[test]
fn each_resource_boundary_is_measured_once_and_rechecked_at_store_admission() {
    let fixture = Fixture::new(&[0, 1], 0, 2);
    let packet = fixture.packet(PacketBudget::default()).unwrap();
    let r = &packet.resources;
    let exact = PacketBudget {
        max_bytes: r.total_bytes,
        max_value_bytes: r.max_value_bytes,
        max_nodes: r.nodes,
        max_transactions: r.transactions,
        max_transaction_bytes: r.max_transaction_bytes,
        max_access_keys: r.access_keys,
        max_receipt_bytes: r.max_receipt_bytes,
    };
    assert_eq!(
        packet.record_bytes(),
        packet
            .records
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>()
    );
    packet.validate_budget(exact).unwrap();
    fixture.packet(exact).unwrap();
    load(&packet, &packet.records, exact).unwrap().unwrap();
    let mut rejected = Vec::new();
    let mut budget = exact;
    budget.max_bytes -= 1;
    rejected.push(budget);
    let mut budget = exact;
    budget.max_value_bytes -= 1;
    rejected.push(budget);
    let mut budget = exact;
    budget.max_nodes -= 1;
    rejected.push(budget);
    let mut budget = exact;
    budget.max_transactions -= 1;
    rejected.push(budget);
    let mut budget = exact;
    budget.max_transaction_bytes -= 1;
    rejected.push(budget);
    let mut budget = exact;
    budget.max_access_keys -= 1;
    rejected.push(budget);
    let mut budget = exact;
    budget.max_receipt_bytes -= 1;
    rejected.push(budget);
    for budget in rejected {
        assert!(packet.validate_budget(budget).is_err());
        assert!(fixture.packet(budget).is_err());
        assert!(load(&packet, &packet.records, budget).is_err());
    }
}

#[test]
fn malformed_counts_and_lengths_fail_before_allocating_declared_content() {
    let budget = PacketBudget::default();
    let bytes = u64::MAX.to_be_bytes();
    assert!(Reader::new(&bytes).count(usize::MAX, 1).is_err());
    assert!(Reader::new(&bytes).frame(usize::MAX).is_err());
    assert!(Reader::new(&0u64.to_be_bytes()).frame(10).is_err());
    let packet = Fixture::new(&[0], 0, 1).packet(budget).unwrap();
    let mut bytes = document(&packet);
    // Full fixed context occupies 308 bytes after magic + candidate id.
    bytes[348..356].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(decode_document(&bytes, packet.candidate_id(), budget).is_err());
    let mut bytes = document(&packet);
    bytes.push(0);
    assert!(decode_document(&bytes, packet.candidate_id(), budget).is_err());
    let original = document(&packet);
    for end in [0, 7, 39, 348, original.len() - 1] {
        assert!(decode_document(&original[..end], packet.candidate_id(), budget).is_err());
    }
}

#[test]
fn independently_changed_context_body_or_commitments_are_rejected() {
    let packet = Fixture::new(&[0], 0, 1)
        .packet(PacketBudget::default())
        .unwrap();
    let original = document(&packet);
    for offset in [
        8, 40, 48, 80, 112, 144, 148, 180, 212, 220, 252, 284, 292, 324, 332, 340,
    ] {
        let mut bytes = original.clone();
        bytes[offset] ^= 1;
        assert!(
            decode_document(&bytes, packet.candidate_id(), PacketBudget::default()).is_err(),
            "offset {offset}"
        );
    }
    let mut reader = Reader::new(&original[40..]);
    decode_plan(&mut reader, PacketBudget::default()).unwrap();
    let suffix = original.len() - reader.remaining.len();
    for offset in [suffix, suffix + 32, suffix + 64] {
        let mut bytes = original.clone();
        bytes[offset] ^= 1;
        assert!(decode_document(&bytes, packet.candidate_id(), PacketBudget::default()).is_err());
    }
}

#[test]
fn receipt_hash_accounts_signer_nonce_fee_and_journal_relations_are_checked() {
    for field in 0..11 {
        let mut fixture = Fixture::new(&[3, 4], 0, 1);
        let r = &mut fixture.receipts[0];
        match field {
            0 => r.tx_hash[0] ^= 1,
            1 => r.delta.tx_hash[0] ^= 1,
            2 => r.signer_identity[0] ^= 1,
            3 => r.delta.nonce_identity.push('a'),
            4 => r.delta.nonce_before += 1,
            5 => r.delta.nonce_after += 1,
            6 => r.delta.payer.account = [9; 20].into(),
            7 => r.delta.recipient.account = [9; 32].into(),
            8 => r.delta.fee_funding_delta = 1,
            9 => r.fee_failure = None,
            10 => r.clear_clearing_candidates = true,
            _ => unreachable!(),
        }
        assert!(
            fixture.packet(PacketBudget::default()).is_err(),
            "field {field}"
        );
    }
    // Individually plausible receipt transitions cannot hide a skipped shared
    // signer nonce merely by using its other balance-account representation.
    assert!(Fixture::new(&[3, 5], 0, 1)
        .packet(PacketBudget::default())
        .is_err());
}

#[test]
fn receipt_codec_refuses_trailing_bytes_and_unknown_variants() {
    let fixture = Fixture::new(&[0], 0, 1);
    let mut receipt = postcard::to_allocvec(&fixture.receipts[0]).unwrap();
    receipt.push(0);
    assert!(validate_receipts(&fixture.plan, &[receipt], PacketBudget::default()).is_err());
    assert!(validate_receipts(&fixture.plan, &[vec![255; 128]], PacketBudget::default()).is_err());
}

#[test]
fn manifest_order_and_content_hash_are_not_accepted_on_trust() {
    let mut fixture = Fixture::new(&[0], 0, 2);
    let hash = *fixture.nodes.keys().next().unwrap();
    fixture.nodes.get_mut(&hash).unwrap()[0] ^= 1;
    assert!(fixture.packet(PacketBudget::default()).is_err());
    let packet = Fixture::new(&[0], 0, 2)
        .packet(PacketBudget::default())
        .unwrap();
    let mut bytes = document(&packet);
    let mut reader = Reader::new(&bytes[40..]);
    decode_plan(&mut reader, PacketBudget::default()).unwrap();
    let manifest = bytes.len() - reader.remaining.len() + 96 + 8;
    let first = bytes[manifest..manifest + 32].to_vec();
    bytes[manifest + 32..manifest + 64].copy_from_slice(&first);
    assert!(decode_document(&bytes, packet.candidate_id(), PacketBudget::default()).is_err());
}
