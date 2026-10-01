// Real authenticated transport plus synchronous pool persistence/reopen. This
// runs inside each existing four-block fixture, not a synthetic delivery count.
#[allow(clippy::too_many_arguments)]
fn exercise_fresh_pool_receipts(
    path: &Path,
    chain: u64,
    genesis: [u8; 32],
    params: &serde_json::Value,
    lifecycle: &mut crate::native_block_seal::service::FreshChainLifecycleV1,
    leader: &crate::product_mainline_overlay::ProductMainlineOverlayRuntimeV1,
    sender: &crate::product_mainline_overlay::ProductMainlineOverlayRuntimeV1,
    expected_hash: [u8; 32],
) {
    use crate::product_mainline_overlay::{
        ProductMainlineOverlayEventV1 as Event, ProductMainlineOverlayPayloadClassV1 as Class,
        ProductMainlineOverlayRecipientAckDispositionV1 as Kind,
    };
    use crate::tx_ingress::fresh_pool::{FreshTransactionPool, PendingTransaction};
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut received = None;
    let mut fresh_ack = None;
    while received.is_none() || fresh_ack.is_none() {
        for event in sender.drain_events(128) {
            match event {
                Event::Inbound(inbound) if inbound.payload_class == Class::NativeTransaction => {
                    assert_eq!(inbound.object_hash, expected_hash);
                    received = Some(inbound);
                }
                Event::RecipientAck { ack, .. }
                    if ack.disposition == Kind::PendingTransactionPersisted =>
                {
                    // Neither wrong-chain nor rate-dropped input was admitted.
                    assert_eq!(ack.object_hash, expected_hash);
                    assert_eq!(ack.recipient_peer_id, leader.startup().local_peer_id);
                    assert_eq!(ack.original_sender_peer_id, sender.startup().local_peer_id);
                    fresh_ack = Some(ack);
                }
                _ => {}
            }
        }
        assert!(
            Instant::now() < deadline,
            "fresh durable receipt/gossip deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let inbound = received.unwrap();
    let pool_path = path.with_extension("fresh-receipt-peer-pool");
    assert!(!pool_path.exists());
    let open = || FreshTransactionPool::open(&pool_path, chain, genesis, params).unwrap();
    let mut pool = open();
    let entry =
        PendingTransaction::authenticate(inbound.frame.payload.clone(), chain, params).unwrap();
    assert_eq!(entry.hash, expected_hash);
    assert!(pool.insert(entry).unwrap());
    let sequence = pool.write_sequence_for_test();
    drop(pool);
    let pool = open();
    assert_eq!(pool.get(&expected_hash).unwrap().raw, inbound.frame.payload);
    assert_eq!(pool.write_sequence_for_test(), sequence);
    assert!(sender.try_submit_pending_transaction_ack(&inbound).unwrap());
    let before = lifecycle.status_json();
    assert_eq!(
        before["transaction_transport"]["recipient_acks_accepted"],
        0
    );
    let mut accepted = false;
    while !accepted {
        for event in leader.drain_events(128) {
            if let Event::RecipientAck {
                ack,
                metric_peer_id,
            } = &event
            {
                if ack.disposition != Kind::PendingTransactionPersisted {
                    continue;
                }
                assert_eq!(ack.object_hash, expected_hash);
                for field in 0..4 {
                    let mut invalid = ack.clone();
                    match field {
                        0 => invalid.signature[0] ^= 1,
                        1 => invalid.chain_id += 1,
                        2 => invalid.object_hash[0] ^= 1,
                        _ => invalid.disposition = Kind::JournalPersisted,
                    }
                    lifecycle.observe_transaction_transport_event(&Event::RecipientAck {
                        ack: invalid,
                        metric_peer_id: *metric_peer_id,
                    });
                }
                assert_eq!(
                    lifecycle.status_json()["transaction_transport"]["recipient_acks_accepted"],
                    0
                );
                lifecycle.observe_transaction_transport_event(&event);
                accepted = true;
            } else {
                lifecycle.observe_transaction_transport_event(&event);
            }
        }
        assert!(
            Instant::now() < deadline,
            "fresh receipt consumption deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let after = lifecycle.status_json();
    assert_eq!(after["transaction_transport"]["recipient_acks_accepted"], 1);
    assert_eq!(after["transaction_transport"]["acked_peer_transactions"], 1);
    for field in [
        "height",
        "durable_pending_transactions",
        "decision_confirmed",
        "proposed_successors",
    ] {
        assert_eq!(after[field], before[field], "ACK changed {field}");
    }
    lifecycle.observe_transaction_transport_event(&Event::E2eSessionEstablished {
        remote_peer_id: sender.startup().local_peer_id.clone(),
    });
    assert_eq!(
        lifecycle.status_json()["transaction_transport"]["acked_peer_transactions"],
        0
    );
    assert_eq!(pool.write_sequence_for_test(), sequence);
}
