mod pending_coalescing {
    use super::*;

    fn budget() -> Arc<Mutex<ProductMainlineOverlayPendingBudgetV1>> {
        Arc::new(Mutex::new(ProductMainlineOverlayPendingBudgetV1::new(
            ProductMainlineOverlayResourceLimitsV1 {
                pending_per_peer_count: 16,
                pending_per_peer_bytes: 64,
                pending_total_count: 32,
                pending_total_bytes: 128,
                ..ProductMainlineOverlayResourceLimitsV1::default()
            },
        )))
    }

    fn transaction(
        hash: u8,
        enqueued: u64,
        expires: u64,
    ) -> Arc<ProductMainlineOverlayOutboundItemV1> {
        let mut item = outbound_item_v1(
            ProductMainlineOverlayPayloadClassV1::NativeTransaction,
            [hash; 32],
            vec![1, 2, 3, 4],
        );
        item.enqueued_at_ms = enqueued;
        item.expires_at_ms = expires;
        Arc::new(item)
    }

    fn push(
        budget: &Arc<Mutex<ProductMainlineOverlayPendingBudgetV1>>,
        queue: &mut VecDeque<ProductMainlineOverlayPendingV1>,
        peer: &str,
        item: Arc<ProductMainlineOverlayOutboundItemV1>,
        now_ms: u64,
    ) {
        let permit = try_reserve_pending_fanout_v1(budget, &[peer.into()], item.payload.len())
            .unwrap()
            .remove(peer)
            .unwrap();
        push_pending_outbound_v1(queue, item, permit, now_ms);
    }

    fn assert_usage(
        budget: &Arc<Mutex<ProductMainlineOverlayPendingBudgetV1>>,
        count: usize,
        bytes: usize,
    ) {
        let state = budget.lock().unwrap();
        assert_eq!(state.total.count, count);
        assert_eq!(state.total.bytes, bytes);
        assert_eq!(
            state
                .by_peer
                .values()
                .map(|usage| usage.count)
                .sum::<usize>(),
            count
        );
        assert_eq!(
            state
                .by_peer
                .values()
                .map(|usage| usage.bytes)
                .sum::<usize>(),
            bytes
        );
    }

    #[test]
    fn single_pending_duplicates_release_budget_without_moving_fifo_or_extending_ttl() {
        let budget = budget();
        let mut queue = VecDeque::new();
        let first = transaction(1, 10, 20);
        let following = transaction(2, 11, 21);
        push(&budget, &mut queue, "peer-a", Arc::clone(&first), 10);
        push(&budget, &mut queue, "peer-a", Arc::clone(&following), 11);
        for now in 12..20 {
            push(
                &budget,
                &mut queue,
                "peer-a",
                transaction(1, now, now + 100),
                now,
            );
            assert_usage(&budget, 2, 8);
            assert!(Arc::ptr_eq(&queue[0].item, &first));
            assert!(Arc::ptr_eq(&queue[1].item, &following));
            assert_eq!(queue[0].item.enqueued_at_ms, 10);
            assert_eq!(queue[0].item.expires_at_ms, 20);
        }
        assert!(queue[0].item.expired_at(20));
        drop(queue);
        assert_usage(&budget, 0, 0);
    }

    #[test]
    fn mesh_pending_fanout_coalesces_per_peer_and_allows_retry_after_admission() {
        let budget = budget();
        let peers = vec!["peer-a".to_string(), "peer-b".to_string()];
        let mut queues: BTreeMap<_, VecDeque<_>> = peers
            .iter()
            .map(|peer| (peer.clone(), VecDeque::new()))
            .collect();
        let first = transaction(1, 10, 100);
        for item in [Arc::clone(&first), transaction(1, 11, 101)] {
            let reservations = try_reserve_pending_fanout_v1(&budget, &peers, 4).unwrap();
            for (peer, permit) in reservations {
                push_pending_outbound_v1(
                    queues.get_mut(&peer).unwrap(),
                    Arc::clone(&item),
                    permit,
                    11,
                );
            }
            assert_usage(&budget, 2, 8);
        }
        for queue in queues.values() {
            assert_eq!(queue.len(), 1);
            assert!(Arc::ptr_eq(&queue[0].item, &first));
        }
        // The production sender pops only after relay admission, not recipient
        // delivery. That pop must leave no suppression state for a later retry.
        drop(queues.get_mut("peer-a").unwrap().pop_front().unwrap());
        assert_usage(&budget, 1, 4);
        let retry = transaction(1, 12, 102);
        let reservations = try_reserve_pending_fanout_v1(&budget, &peers, 4).unwrap();
        for (peer, permit) in reservations {
            push_pending_outbound_v1(
                queues.get_mut(&peer).unwrap(),
                Arc::clone(&retry),
                permit,
                12,
            );
        }
        assert!(Arc::ptr_eq(&queues["peer-a"][0].item, &retry));
        assert!(Arc::ptr_eq(&queues["peer-b"][0].item, &first));
        assert_usage(&budget, 2, 8);
        drop(queues);
        assert_usage(&budget, 0, 0);
    }

    #[test]
    fn expired_pending_cannot_swallow_a_fresh_retry_at_the_expiry_boundary() {
        let budget = budget();
        let mut queue = VecDeque::new();
        let expired = transaction(1, 10, 20);
        let fresh = transaction(1, 20, 30);
        push(&budget, &mut queue, "peer-a", Arc::clone(&expired), 10);
        push(&budget, &mut queue, "peer-a", Arc::clone(&fresh), 20);
        assert_eq!(queue.len(), 2);
        assert!(Arc::ptr_eq(&queue[0].item, &expired));
        assert!(Arc::ptr_eq(&queue[1].item, &fresh));
        assert_usage(&budget, 2, 8);
        // The existing expiry path removes/reports the old item independently.
        drop(queue.pop_front().unwrap());
        assert!(Arc::ptr_eq(&queue[0].item, &fresh));
        assert_usage(&budget, 1, 4);
    }

    #[test]
    fn pending_dedup_requires_peer_class_hash_digest_and_full_payload_equality() {
        let budget = budget();
        let mut queue = VecDeque::new();
        push(&budget, &mut queue, "peer-a", transaction(1, 10, 100), 10);
        // Explicit peer comparison is defensive even though production keeps
        // separate queues. Identical bytes for another destination cannot fold.
        push(&budget, &mut queue, "peer-b", transaction(1, 10, 100), 10);
        push(&budget, &mut queue, "peer-a", transaction(2, 10, 100), 10);

        let mut other_digest = transaction(1, 10, 100);
        Arc::get_mut(&mut other_digest).unwrap().payload_sha256 = [9; 32];
        push(&budget, &mut queue, "peer-a", other_digest, 10);

        // Deliberately retain the old digest: full bytes must still be checked,
        // never trust the precomputed metadata alone for dropping a submission.
        let mut other_bytes = transaction(1, 10, 100);
        Arc::get_mut(&mut other_bytes).unwrap().payload = Arc::from(vec![4, 3, 2, 1]);
        push(&budget, &mut queue, "peer-a", other_bytes, 10);

        let mut seal = transaction(1, 10, 100);
        Arc::get_mut(&mut seal).unwrap().payload_class =
            ProductMainlineOverlayPayloadClassV1::NativeSeal;
        push(&budget, &mut queue, "peer-a", Arc::clone(&seal), 10);
        push(&budget, &mut queue, "peer-a", seal, 10);
        assert_eq!(queue.len(), 7);
        assert_usage(&budget, 7, 28);
        drop(queue);
        assert_usage(&budget, 0, 0);

        // Nor can a queued seal suppress a following native transaction.
        let mut seal = transaction(1, 10, 100);
        Arc::get_mut(&mut seal).unwrap().payload_class =
            ProductMainlineOverlayPayloadClassV1::NativeSeal;
        let mut queue = VecDeque::new();
        push(&budget, &mut queue, "peer-a", seal, 10);
        push(&budget, &mut queue, "peer-a", transaction(1, 10, 100), 10);
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn reconnect_retains_original_pending_but_does_not_suppress_later_resubmission() {
        let budget = budget();
        let mut queue = VecDeque::new();
        let first = transaction(1, 10, 100);
        push(&budget, &mut queue, "peer-a", Arc::clone(&first), 10);
        // Single-peer send failure restores the exact pending item at the
        // front; mesh failures leave it in place. Neither resets its lifetime.
        let failed_send = queue.pop_front().unwrap();
        queue.push_front(failed_send);
        push(&budget, &mut queue, "peer-a", transaction(1, 20, 110), 20);
        assert_eq!(queue.len(), 1);
        assert!(Arc::ptr_eq(&queue[0].item, &first));
        assert_usage(&budget, 1, 4);
        drop(queue.pop_front().unwrap());
        assert_usage(&budget, 0, 0);
        let retry = transaction(1, 30, 120);
        push(&budget, &mut queue, "peer-a", Arc::clone(&retry), 30);
        assert!(Arc::ptr_eq(&queue[0].item, &retry));
        assert_usage(&budget, 1, 4);
    }
}
