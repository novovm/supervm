use super::*;
use crate::persistence::{OpenMode, PacketBudget, StorageDomain, StoreConfig};
use crate::state::tree::{
    empty_root, read_state_value, stage_state_update, NodeHash, StateChange, StateNodeReader,
};
use novovm_aoem::StorageConfig;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn state(validator: u8) -> MetaKey {
    MetaKey::ConsensusState([validator; 32])
}
fn outbox(validator: u8, sequence: u64) -> MetaKey {
    MetaKey::ConsensusOutbox {
        validator: [validator; 32],
        sequence,
    }
}
fn change(key: MetaKey, expected: Option<&[u8]>, value: &[u8]) -> MetaChange {
    MetaChange {
        key,
        expected: expected.map(<[u8]>::to_vec),
        value: value.to_vec(),
    }
}

#[test]
fn keys_are_closed_separated_and_sequence_is_fixed_width() {
    let keys = [
        state(1),
        state(2),
        outbox(1, 0),
        outbox(1, 1),
        outbox(1, u64::MAX),
        outbox(2, 0),
    ];
    let encoded: BTreeSet<_> = keys.iter().map(MetaKey::relative_key).collect();
    assert_eq!(encoded.len(), keys.len());
    for key in &encoded {
        assert!(key.starts_with(b"m/consensus/v1/"));
        assert!(![b'n', b'd', b'c'].contains(&key[0]));
    }
    assert_eq!(
        state(1).relative_key(),
        [b"m/consensus/v1/state/".as_slice(), &[1; 32]].concat()
    );
    let one = outbox(1, 1).relative_key();
    assert_eq!(&one[one.len() - 8..], &1u64.to_be_bytes());
    assert_eq!(one.len(), outbox(1, u64::MAX).relative_key().len());
}

#[test]
fn construction_bounds_expected_and_new_bytes_without_weakening_append_only_keys() {
    assert!(MetaTransition::new(Vec::new()).is_err());
    let nine = (0..9)
        .map(|index| change(state(index), None, b"v"))
        .collect();
    assert!(MetaTransition::new(nine).is_err());
    let eight = (0..8)
        .map(|index| change(state(index), None, b"v"))
        .collect();
    assert!(MetaTransition::new(eight).is_ok());
    assert!(MetaTransition::new(vec![
        change(state(1), None, b"v"),
        change(state(1), Some(b"v"), b"w")
    ])
    .is_err());
    assert!(MetaTransition::new(vec![change(outbox(1, 0), Some(b"old"), b"new")]).is_err());
    assert!(MetaTransition::new(vec![change(outbox(1, 0), Some(b""), b"")]).is_err());
    assert!(MetaTransition::new(vec![change(outbox(1, 0), None, b"")]).is_ok());
    let max = vec![1; MAX_VALUE_BYTES];
    let too_big = vec![1; MAX_VALUE_BYTES + 1];
    assert!(MetaTransition::new(vec![change(state(1), None, &too_big)]).is_err());
    assert!(MetaTransition::new(vec![change(state(1), Some(&too_big), b"ok")]).is_err());
    let exact = MetaTransition::new(vec![change(state(1), Some(&max), &max)]).unwrap();
    assert_eq!(
        exact.retained_bytes(),
        MAX_TRANSITION_BYTES + state(1).relative_key().len()
    );
    assert!(MetaTransition::new(vec![
        change(state(1), Some(&max), &max),
        change(state(2), None, b"x")
    ])
    .is_err());
}

fn directory(label: &str) -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/metadata-tests")
        .join(format!("{label}-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

fn config(directory: &Path) -> Result<StoreConfig> {
    Ok(StoreConfig {
        library: std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted NOVOVM_AOEM_TEST_LIBRARY required")?
            .into(),
        database: directory.join("same-store.rocksdb"),
        domain: StorageDomain {
            chain_id: 491,
            genesis_config_commitment: [1; 32],
            protocol_commitment: [2; 32],
        },
        storage: StorageConfig::default(),
        packet_budget: PacketBudget::default(),
    })
}

struct Empty;
impl StateNodeReader for Empty {
    fn read_node(&self, _: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; local metadata CAS, not finality"]
fn real_metadata_same_database_atomic_compare_replay_and_append_only_outbox() -> Result<()> {
    let directory = directory("cas")?;
    let config = config(&directory)?;
    let store = CandidateStore::open(config.clone(), OpenMode::CreateNew)?;
    let update = stage_state_update(
        &Empty,
        empty_root(),
        &[StateChange::Put {
            key: b"account".to_vec(),
            value: b"unchanged".to_vec(),
        }],
    )?;
    store.install_unpublished_state(&update)?;
    let keys = [state(1), outbox(1, 0)];
    assert_eq!(store.read_metadata(&keys)?.values, vec![None, None]);
    assert!(store.read_metadata(&[]).is_err());
    assert!(store.read_metadata(&vec![state(1); 9]).is_err());
    let first = MetaTransition::new(vec![
        change(keys[0].clone(), None, b"state-0"),
        change(keys[1].clone(), None, b"vote-0"),
    ])?;
    assert_eq!(store.apply_metadata(&first)?, MetaOutcome::Applied);
    assert_eq!(store.apply_metadata(&first)?, MetaOutcome::AlreadyPresent);
    assert_eq!(
        store.read_metadata(&keys)?.values,
        vec![Some(b"state-0".to_vec()), Some(b"vote-0".to_vec())]
    );
    let next = MetaTransition::new(vec![
        change(state(1), Some(b"state-0"), b"state-1"),
        change(outbox(1, 1), None, b"vote-1"),
    ])?;
    assert_eq!(store.apply_metadata(&next)?, MetaOutcome::Applied);
    assert_eq!(store.apply_metadata(&next)?, MetaOutcome::AlreadyPresent);
    // A stale prefix cannot overwrite the newer state, even though its outbox
    // element already equals new. No subset may be silently replayed/applied.
    assert_eq!(store.apply_metadata(&first)?, MetaOutcome::Conflict);
    let overwrite = MetaTransition::new(vec![change(outbox(1, 0), None, b"different-vote")])?;
    assert_eq!(store.apply_metadata(&overwrite)?, MetaOutcome::Conflict);
    assert_eq!(
        store
            .read_metadata(&[outbox(1, 1), state(1), outbox(1, 0), state(1)])?
            .values,
        vec![
            Some(b"vote-1".to_vec()),
            Some(b"state-1".to_vec()),
            Some(b"vote-0".to_vec()),
            Some(b"state-1".to_vec())
        ]
    );
    assert!(
        !store.is_write_frozen(),
        "ordinary CAS conflict is not an uncertain write"
    );
    assert_eq!(
        read_state_value(&store, update.root(), b"account")?,
        Some(b"unchanged".to_vec())
    );
    drop(store);
    let reopened = CandidateStore::open(config.clone(), OpenMode::Existing)?;
    assert_eq!(reopened.apply_metadata(&next)?, MetaOutcome::AlreadyPresent);
    assert_eq!(
        reopened.read_metadata(&[state(1), outbox(1, 1)])?.values,
        vec![Some(b"state-1".to_vec()), Some(b"vote-1".to_vec())]
    );
    assert_eq!(
        read_state_value(&reopened, update.root(), b"account")?,
        Some(b"unchanged".to_vec())
    );
    let original_scoped = reopened.scoped_key(&state(1).relative_key());
    drop(reopened);

    // The SAME physical database cannot be reopened under another chain domain.
    let mut wrong = config.clone();
    wrong.domain.genesis_config_commitment[0] ^= 1;
    assert!(CandidateStore::open(wrong.clone(), OpenMode::Existing).is_err());
    // A distinct store with that other domain has independent metadata names.
    wrong.database = directory.join("other-domain.rocksdb");
    let isolated = CandidateStore::open(wrong, OpenMode::CreateNew)?;
    assert_ne!(
        isolated.scoped_key(&state(1).relative_key()),
        original_scoped
    );
    assert_eq!(isolated.read_metadata(&[state(1)])?.values, vec![None]);
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; genuine mixed CAS rejection"]
fn real_partial_replay_never_completes_missing_outbox_or_mutates_other_keys() -> Result<()> {
    let config = config(&directory("mixed")?)?;
    let store = CandidateStore::open(config, OpenMode::CreateNew)?;
    let put = MetaTransition::new(vec![change(state(7), None, b"old")])?;
    assert_eq!(store.apply_metadata(&put)?, MetaOutcome::Applied);
    let state_only = MetaTransition::new(vec![change(state(7), Some(b"old"), b"new")])?;
    assert_eq!(store.apply_metadata(&state_only)?, MetaOutcome::Applied);
    let mixed = MetaTransition::new(vec![
        change(state(7), Some(b"old"), b"new"),
        change(outbox(7, 1), None, b"must-not-appear"),
    ])?;
    assert_eq!(store.apply_metadata(&mixed)?, MetaOutcome::Conflict);
    assert_eq!(
        store.read_metadata(&[state(7), outbox(7, 1)])?.values,
        vec![Some(b"new".to_vec()), None]
    );
    let empty = MetaTransition::new(vec![change(state(8), None, b"")])?;
    assert_eq!(store.apply_metadata(&empty)?, MetaOutcome::Applied);
    let absent_expectation = MetaTransition::new(vec![change(state(8), None, b"nonempty")])?;
    assert_eq!(
        store.apply_metadata(&absent_expectation)?,
        MetaOutcome::Conflict
    );
    let empty_expectation = MetaTransition::new(vec![change(state(8), Some(b""), b"nonempty")])?;
    assert_eq!(
        store.apply_metadata(&empty_expectation)?,
        MetaOutcome::Applied
    );
    assert!(!store.is_write_frozen());
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; actual corruption freezes writes"]
fn real_oversized_metadata_is_rejected_and_cannot_be_repaired_by_transition() -> Result<()> {
    let config = config(&directory("corrupt")?)?;
    let store = CandidateStore::open(config, OpenMode::CreateNew)?;
    // Private test access injects physical corruption through the real provider;
    // no production raw-write API is introduced by metadata.
    let key = state(9);
    store
        .storage
        .borrow_mut()
        .atomic_write_batch(&[StorageWrite::Put {
            key: store.scoped_key(&key.relative_key()),
            value: vec![0; MAX_VALUE_BYTES + 1],
        }])?;
    assert!(store.read_metadata(std::slice::from_ref(&key)).is_err());
    assert!(store.is_write_frozen());
    let attempt = MetaTransition::new(vec![change(key, None, b"replacement")])?;
    assert!(store.apply_metadata(&attempt).is_err());
    Ok(())
}
