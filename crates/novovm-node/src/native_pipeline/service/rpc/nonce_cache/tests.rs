use super::super::{
    advance_nonce_window, nonce_selection_window, select_pending_batch,
    select_prepared_nonce_window, AdmissionInput, Pending,
};
use super::*;

enum Completion {
    Pending,
    Ready(Option<Vec<u8>>),
    Failed,
}

impl NonceReadTicket for Completion {
    fn try_take_nonce(&mut self) -> Result<Option<Option<Vec<u8>>>> {
        match self {
            Self::Pending => Ok(None),
            Self::Ready(value) => Ok(Some(value.take())),
            Self::Failed => anyhow::bail!("test I/O disconnected"),
        }
    }
}

fn parent(height: u64) -> ParentPoint {
    ParentPoint {
        height,
        block_hash: subject(height),
        state_root: subject(height + 100),
        receipt_batch_commitment: subject(height + 200),
        state_version: height * 2,
        decision_hash: subject(height + 300),
    }
}

fn subject(index: u64) -> Hash {
    let mut bytes = [0; 32];
    bytes[..8].copy_from_slice(&index.to_le_bytes());
    bytes
}

fn cache(capacity: usize) -> NonceCache<Completion> {
    NonceCache::new(capacity)
}

#[test]
fn uca_cache_requires_initialized_nonce_even_after_eviction() -> Result<()> {
    let mut cache = cache(1);
    cache.require_initialized_subjects();
    let parent = parent(3);
    cache.bind_current_parent(parent);
    let owner = subject(1);
    assert!(cache.accept_read(parent, owner, None).is_err());
    assert_eq!(cache.lookup(&owner), Lookup::Unknown);
    cache.accept_read(parent, owner, Some(0u64.to_le_bytes().to_vec()))?;
    assert_eq!(cache.lookup(&owner), Lookup::Present(0));
    cache.published(subject(2), 9);
    assert_eq!(cache.lookup(&owner), Lookup::Unknown);
    assert!(cache.accept_read(parent, owner, None).is_err());
    assert_eq!(cache.lookup(&owner), Lookup::Unknown);
    Ok(())
}

fn pending(subject: Hash, nonces: &[u64]) -> (VecDeque<Hash>, BTreeMap<Hash, Pending>) {
    let mut order = VecDeque::new();
    let mut pending = BTreeMap::new();
    for (index, nonce) in nonces.iter().enumerate() {
        let hash = self::subject(index as u64 + 1_000);
        order.push_back(hash);
        pending.insert(
            hash,
            Pending {
                input: AdmissionInput::Raw(vec![0; 8]),
                admitted: None,
                canonical_len: 8,
                signer: subject,
                nonce: *nonce,
                gossip_missing: 0,
            },
        );
    }
    (order, pending)
}

#[test]
fn cache_eviction_is_not_a_chain_lifetime_subject_limit() {
    let mut cache = cache(3);
    cache.bind_current_parent(parent(0));
    // Finite pure cache regression crosses the old 65,536-subject failure;
    // this is not a second 65K-signature/process throughput test.
    for index in 0..65_540 {
        cache.published(subject(index), index + 1);
        assert!(cache.values.len() <= 3);
        assert!(cache.insertion_order.len() <= 3);
        assert!(cache.absent.len() <= 3);
    }
    assert_eq!(cache.lookup(&subject(0)), Lookup::Unknown);
    assert_eq!(cache.lookup(&subject(65_539)), Lookup::Present(65_540));
    assert_eq!(cache.values.len(), 3);
}

#[test]
fn evicted_old_subject_waits_for_real_parent_nonce_then_rejects_replay() -> Result<()> {
    let mut cache = cache(2);
    let parent = parent(3);
    cache.bind_current_parent(parent);
    let old = subject(1);
    cache.published(old, 9);
    cache.published(subject(2), 1);
    cache.published(subject(3), 1);
    assert_eq!(cache.lookup(&old), Lookup::Unknown);

    let (order, pending) = pending(old, &[0, 9, 9, 10]);
    assert!(select_pending_batch(&order, &pending, &cache.values, BTreeMap::new(), 8).is_empty());
    cache.poll(parent, [old].into_iter(), |root, id| {
        assert_eq!(root, parent.state_root);
        assert_eq!(id, old);
        Ok(Some(Completion::Pending))
    })?;
    assert_eq!(cache.lookup(&old), Lookup::Reading);
    assert!(!cache.poll(parent, [old].into_iter(), |_, _| {
        panic!("a still-pending subject must not be submitted twice")
    })?);
    assert!(select_pending_batch(&order, &pending, &cache.values, BTreeMap::new(), 8).is_empty());

    cache.reads[0].ticket = Completion::Ready(Some(9_u64.to_le_bytes().to_vec()));
    assert!(cache.poll(parent, [old].into_iter(), |_, _| panic!("resolved read"))?);
    assert_eq!(cache.lookup(&old), Lookup::Present(9));
    // Nonce zero stays stale, the duplicate nine is skipped, then ten follows.
    assert_eq!(
        select_pending_batch(&order, &pending, &cache.values, BTreeMap::new(), 8),
        vec![order[1], order[3]]
    );
    assert_eq!(cache.get(&old), Some(&9));
    assert_eq!(pending.len(), 4, "selection does not erase accepted inputs");
    Ok(())
}

#[test]
fn missing_unready_absent_and_present_zero_are_distinct() -> Result<()> {
    let mut cache = cache(3);
    let parent = parent(0);
    let id = subject(10);
    assert!(!cache.poll(parent, [id].into_iter(), |_, _| Ok(None))?);
    assert_eq!(cache.lookup(&id), Lookup::Unknown);
    assert_eq!(cache.get(&id), None);
    cache.poll(parent, [id].into_iter(), |_, _| {
        Ok(Some(Completion::Pending))
    })?;
    assert_eq!(cache.lookup(&id), Lookup::Reading);
    assert_eq!(cache.get(&id), None);
    cache.reads[0].ticket = Completion::Ready(None);
    cache.poll(parent, [id].into_iter(), |_, _| panic!("proved absent"))?;
    assert_eq!(cache.lookup(&id), Lookup::Absent);
    assert_eq!(cache.get(&id), Some(&0));

    let present = subject(11);
    cache.accept_read(parent, present, Some(0_u64.to_le_bytes().to_vec()))?;
    assert_eq!(cache.lookup(&present), Lookup::Present(0));
    // No wildcard default is involved; zero selection requires a resolved read.
    let (order, pending) = pending(id, &[0, 1]);
    assert_eq!(
        select_pending_batch(&order, &pending, &cache.values, BTreeMap::new(), 8),
        order.into_iter().collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn read_errors_and_malformed_nonce_records_never_mean_absence() -> Result<()> {
    let mut cache = cache(2);
    let parent = parent(1);
    let id = subject(1);
    cache.poll(parent, [id].into_iter(), |_, _| {
        Ok(Some(Completion::Failed))
    })?;
    assert!(cache
        .poll(parent, [id].into_iter(), |_, _| panic!("failed ticket"))
        .is_err());
    assert_eq!(cache.lookup(&id), Lookup::Unknown);
    for invalid in [vec![], vec![0; 7], vec![0; 9]] {
        assert!(cache.accept_read(parent, id, Some(invalid)).is_err());
        assert_eq!(cache.lookup(&id), Lookup::Unknown);
    }
    assert!(cache
        .poll(parent, [id].into_iter(), |_, _| {
            anyhow::bail!("submission owner closed")
        })
        .is_err());
    assert_eq!(cache.get(&id), None);
    Ok(())
}

#[test]
fn every_parent_binding_change_discards_old_reads_and_values() -> Result<()> {
    let old = parent(4);
    let changed = [
        ParentPoint { height: 5, ..old },
        ParentPoint {
            state_root: subject(77),
            ..old
        },
        ParentPoint {
            state_version: old.state_version + 1,
            ..old
        },
        ParentPoint {
            block_hash: subject(78),
            ..old
        },
        ParentPoint {
            receipt_batch_commitment: subject(79),
            ..old
        },
        ParentPoint {
            decision_hash: subject(80),
            ..old
        },
    ];
    for new in changed {
        let mut cache = cache(2);
        cache.bind_current_parent(old);
        cache.published(subject(1), 7);
        cache.poll(old, [subject(2)].into_iter(), |_, _| {
            Ok(Some(Completion::Pending))
        })?;
        assert!(cache.has_pending_reads());
        cache.bind_current_parent(new);
        assert!(cache.values.is_empty());
        assert!(cache.reads.is_empty());
        assert!(!cache.has_pending_reads());
        assert!(cache
            .accept_read(old, subject(2), Some(99_u64.to_le_bytes().to_vec()))
            .is_err());
        assert_eq!(cache.lookup(&subject(2)), Lookup::Unknown);
        cache.accept_read(new, subject(2), Some(8_u64.to_le_bytes().to_vec()))?;
        assert_eq!(cache.lookup(&subject(2)), Lookup::Present(8));
    }
    Ok(())
}

#[test]
fn published_prefix_updates_preserve_untouched_values_but_not_old_root_tickets() -> Result<()> {
    let mut cache = cache(3);
    let old = parent(7);
    cache.bind_current_parent(old);
    cache.published(subject(1), 7);
    cache.published(subject(2), 11);
    cache.poll(old, [subject(3)].into_iter(), |_, _| {
        Ok(Some(Completion::Pending))
    })?;
    let new = parent(8);
    cache.begin_published_parent(new);
    cache.published(subject(1), 8);
    assert_eq!(cache.lookup(&subject(1)), Lookup::Present(8));
    assert_eq!(cache.lookup(&subject(2)), Lookup::Present(11));
    assert_eq!(cache.lookup(&subject(3)), Lookup::Unknown);
    assert!(cache.reads.is_empty());
    assert!(cache
        .accept_read(old, subject(1), Some(7_u64.to_le_bytes().to_vec()))
        .is_err());
    assert_eq!(cache.lookup(&subject(1)), Lookup::Present(8));
    // A restart/noncontiguous recovery point cannot retain a skipped prefix.
    cache.begin_published_parent(parent(10));
    assert!(cache.values.is_empty());
    Ok(())
}

#[test]
fn state_reads_are_bounded_and_never_replaced_by_pending_nonce_claims() -> Result<()> {
    let mut cache = cache(2);
    let parent = parent(1);
    let subjects: Vec<_> = (0..32).map(subject).collect();
    let mut submissions = 0;
    cache.poll(parent, subjects.iter().copied(), |root, _| {
        assert_eq!(root, parent.state_root);
        submissions += 1;
        Ok(Some(Completion::Pending))
    })?;
    assert_eq!(submissions, MAX_READS);
    assert_eq!(cache.reads.len(), MAX_READS);
    assert!(cache.values.is_empty());
    assert!(!cache.poll(parent, subjects.into_iter(), |_, _| panic!("read bound"))?);
    assert!(cache.values.is_empty());
    let (order, pending) = pending(subject(0), &[0, 1]);
    assert!(select_pending_batch(&order, &pending, &cache.values, BTreeMap::new(), 8).is_empty());
    // This explicit overlay comes from the already-executed durable parent;
    // selection may use it privately, but does not promote it into the cache.
    assert_eq!(
        select_pending_batch(
            &order,
            &pending,
            &cache.values,
            BTreeMap::from([(subject(0), 1)]),
            8
        ),
        vec![order[1]]
    );
    assert!(cache.values.is_empty());
    Ok(())
}

#[test]
fn cold_1024_subject_candidate_waits_for_window_not_eight_row_blocks() -> Result<()> {
    let mut cache = cache(1024);
    let parent = parent(2);
    let mut order = VecDeque::new();
    let mut pending = BTreeMap::new();
    for index in 0..1024 {
        let id = subject(index);
        let hash = subject(index + 10_000);
        order.push_back(hash);
        pending.insert(
            hash,
            Pending {
                input: AdmissionInput::Raw(vec![0; 8]),
                admitted: None,
                canonical_len: 8,
                signer: id,
                nonce: 0,
                gossip_missing: 0,
            },
        );
    }
    let window = nonce_selection_window(&order, 0, 1024);
    assert_eq!(window.len(), 1024);
    let mut submissions = 0;
    let mut prepared_cleanup_turns = 0;
    // Phase zero submits eight; each following phase completes the previous
    // eight. The real selection helper MUST remain unprepared until all 1,024
    // required nonces have completed, without advancing to another pool window.
    for phase in 0..=128 {
        cache.poll(
            parent,
            window.iter().map(|hash| pending[hash].signer),
            |root, _| {
                assert_eq!(root, parent.state_root);
                submissions += 1;
                Ok(Some(Completion::Ready(None)))
            },
        )?;
        assert!(cache.reads.len() <= MAX_READS);
        let selection =
            select_prepared_nonce_window(&window, &pending, &cache.values, BTreeMap::new(), 1024);
        if selection.is_some() {
            // The actual caller places stale-pool cleanup after this readiness
            // gate, never in the per-eight-ticket read/completion operation.
            prepared_cleanup_turns += 1;
        }
        if phase < 128 {
            assert!(cache.has_pending_reads());
            assert!(selection.is_none(), "phase {phase} offered a partial block");
            assert_eq!(prepared_cleanup_turns, 0);
        } else {
            assert!(!cache.has_pending_reads());
            assert_eq!(selection, Some(order.iter().copied().collect()));
        }
    }
    assert_eq!(
        submissions, 1024,
        "no cursor skips or duplicate state reads"
    );
    assert_eq!(cache.values.len(), 1024);
    assert_eq!(prepared_cleanup_turns, 1);
    assert_eq!(pending.len(), 1024, "preparation does not consume inputs");
    Ok(())
}

#[test]
fn published_prefetch_cannot_install_a_private_successor_override() -> Result<()> {
    let mut cache = cache(2);
    let parent = parent(2);
    let id = subject(99);
    let (order, pending) = pending(id, &[1]);
    assert!(
        !cache.has_pending_reads(),
        "idle cache needs no extra polling"
    );
    cache.poll(parent, [id].into_iter(), |root, subject| {
        assert_eq!((root, subject), (parent.state_root, id));
        Ok(Some(Completion::Pending))
    })?;
    assert!(cache.has_pending_reads());
    assert!(!cache.poll(parent, [id].into_iter(), |_, _| panic!("duplicate read"))?);
    assert_eq!(cache.get(&id), None, "unfinished does not mean nonce zero");
    cache.reads[0].ticket = Completion::Ready(None);
    cache.poll(parent, [id].into_iter(), |_, _| panic!("resolved read"))?;
    assert!(!cache.has_pending_reads());
    assert_eq!(cache.lookup(&id), Lookup::Absent);
    assert!(select_pending_batch(&order, &pending, &cache.values, BTreeMap::new(), 2).is_empty());
    assert_eq!(
        select_pending_batch(
            &order,
            &pending,
            &cache.values,
            BTreeMap::from([(id, 1)]),
            2
        ),
        order.iter().copied().collect::<Vec<_>>()
    );
    assert_eq!(
        cache.get(&id),
        Some(&0),
        "private selection is not publication"
    );
    Ok(())
}

#[test]
fn fully_known_gap_window_rotates_to_ready_work_but_unknown_window_does_not() {
    let mut order = VecDeque::new();
    let mut pending = BTreeMap::new();
    let mut known = BTreeMap::new();
    for index in 0..8 {
        let hash = subject(index + 10_000);
        let id = subject(index);
        order.push_back(hash);
        known.insert(id, 0);
        pending.insert(
            hash,
            Pending {
                input: AdmissionInput::Raw(vec![0; 8]),
                admitted: None,
                canonical_len: 8,
                signer: id,
                nonce: if index < 4 { 7 } else { 0 },
                gossip_missing: 0,
            },
        );
    }
    let window = nonce_selection_window(&order, 0, 4);
    assert_eq!(window.len(), 4);
    assert_eq!(
        select_prepared_nonce_window(&window, &pending, &BTreeMap::new(), BTreeMap::new(), 4),
        None,
        "unknown is not proof that this prefix only contains nonce gaps"
    );
    assert_eq!(
        select_prepared_nonce_window(&window, &pending, &known, BTreeMap::new(), 4),
        Some(Vec::new())
    );
    let next = advance_nonce_window(0, window.len(), order.len());
    assert_eq!(next, 4);
    let ready = nonce_selection_window(&order, next, 4);
    assert_eq!(
        select_prepared_nonce_window(&ready, &pending, &known, BTreeMap::new(), 4),
        Some(order.iter().skip(4).copied().collect())
    );
    assert_eq!(advance_nonce_window(next, ready.len(), order.len()), 0);
    assert_eq!(pending.len(), 8);
}
