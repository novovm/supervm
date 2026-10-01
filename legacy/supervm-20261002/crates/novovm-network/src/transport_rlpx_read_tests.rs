struct RegisteredReadSession {
    chain_id: u64,
    peer: NodeId,
}

impl Drop for RegisteredReadSession {
    fn drop(&mut self) {
        eth_fullnode_native_rlpx_sessions_v1()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&(self.chain_id, self.peer.0));
        let _ = unregister_network_runtime_peer(self.chain_id, self.peer.0);
    }
}

fn register_read_test_session(
    chain_id: u64,
    peer: NodeId,
    session: EthFullnodeNativeRlpxLivePeerSessionV1,
) -> RegisteredReadSession {
    let _ = register_network_runtime_peer(chain_id, peer.0);
    observe_network_runtime_eth_peer_status_ok_v1(chain_id, peer.0, Some(120));
    eth_fullnode_native_rlpx_sessions_v1()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert((chain_id, peer.0), session);
    RegisteredReadSession { chain_id, peer }
}

fn has_read_test_session(chain_id: u64, peer: NodeId) -> bool {
    eth_fullnode_native_rlpx_sessions_v1()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .contains_key(&(chain_id, peer.0))
}

#[test]
fn rlpx_read_idle_socket_retains_session_and_decodes_later_ping() {
    let chain_id = 99_290_001;
    let peer = NodeId(99_290_002);
    let (session, mut remote_stream, mut remote_session) = dummy_rlpx_live_session_pair(chain_id);
    let endpoint = session.endpoint.clone();
    let _guard = register_read_test_session(chain_id, peer, session);
    let mut budget = default_eth_fullnode_budget_hooks_v1();
    budget.sync_request_interval_ms = u64::MAX;
    budget.tx_broadcast_interval_ms = u64::MAX;
    remote_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();

    for _ in 0..2 {
        let report = drive_eth_fullnode_native_rlpx_peer_session_once_v1(
            chain_id,
            NodeId(1),
            peer,
            &endpoint,
            &budget,
        )
        .expect("idle socket must not lose its established session");
        assert_eq!(report.inbound_frames, 0);
        assert!(has_read_test_session(chain_id, peer));
    }
    crate::eth_rlpx_write_wire_frame_v1(
        &mut remote_stream,
        &mut remote_session,
        ETH_RLPX_P2P_PING_MSG,
        &[],
    )
    .unwrap();
    let report = drive_eth_fullnode_native_rlpx_peer_session_once_v1(
        chain_id,
        NodeId(1),
        peer,
        &endpoint,
        &budget,
    )
    .unwrap();
    assert_eq!(report.inbound_frames, 1);
    assert!(has_read_test_session(chain_id, peer));
    assert_eq!(
        crate::eth_rlpx_read_wire_frame_v1(&mut remote_stream, &mut remote_session)
            .unwrap()
            .0,
        ETH_RLPX_P2P_PONG_MSG
    );
    let snapshots = snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, &[peer]);
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].decode_failure_count, 0);
    assert_eq!(snapshots[0].timeout_count, 0);
    drop(remote_stream);
    drive_eth_fullnode_native_rlpx_peer_session_once_v1(
        chain_id,
        NodeId(1),
        peer,
        &endpoint,
        &budget,
    )
    .unwrap();
    assert!(
        !has_read_test_session(chain_id, peer),
        "EOF must still close the session"
    );
}

#[test]
fn rlpx_read_incomplete_frame_timeout_evicts_consumed_cipher_session() {
    use std::io::Write;
    for (offset, prefix_size) in [16, 32, 48].into_iter().enumerate() {
        let chain_id = 99_290_010 + offset as u64;
        let peer = NodeId(chain_id + 100);
        let (session, mut remote_stream, mut remote_session) =
            dummy_rlpx_live_session_pair(chain_id);
        let endpoint = session.endpoint.clone();
        let _guard = register_read_test_session(chain_id, peer, session);
        let mut budget = default_eth_fullnode_budget_hooks_v1();
        budget.sync_request_interval_ms = u64::MAX;
        budget.tx_broadcast_interval_ms = u64::MAX;
        let mut wire = Vec::new();
        crate::eth_rlpx_write_wire_frame_v1(
            &mut wire,
            &mut remote_session,
            ETH_RLPX_P2P_PING_MSG,
            b"ping",
        )
        .unwrap();
        remote_stream.write_all(&wire[..prefix_size]).unwrap();
        let error = match drive_eth_fullnode_native_rlpx_peer_session_once_v1(
            chain_id,
            NodeId(1),
            peer,
            &endpoint,
            &budget,
        ) {
            Ok(_) => panic!("incomplete frame must not be treated as idle"),
            Err(error) => error,
        };
        assert_eq!(
            classify_eth_fullnode_peer_failure_v1(&error),
            EthFullnodeNativePeerFailureClassV1::Timeout
        );
        assert!(!has_read_test_session(chain_id, peer));
        let snapshots = snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, &[peer]);
        assert_eq!(snapshots[0].timeout_count, 1);
        assert_eq!(snapshots[0].decode_failure_count, 0);
    }
}

#[test]
fn rlpx_read_idle_does_not_disable_pending_request_deadline() {
    let chain_id = 99_290_020;
    let peer = NodeId(99_290_021);
    let (mut session, _remote_stream, _remote_session) = dummy_rlpx_live_session_pair(chain_id);
    session.last_headers_request_id = Some(1);
    session.last_sync_request_unix_ms = now_unix_ms().saturating_sub(10_000);
    let endpoint = session.endpoint.clone();
    let _guard = register_read_test_session(chain_id, peer, session);
    let mut budget = default_eth_fullnode_budget_hooks_v1();
    budget.rlpx_request_timeout_ms = 1;
    budget.sync_request_interval_ms = u64::MAX;
    budget.tx_broadcast_interval_ms = u64::MAX;
    let result = drive_eth_fullnode_native_rlpx_peer_session_once_v1(
        chain_id,
        NodeId(1),
        peer,
        &endpoint,
        &budget,
    );
    assert!(
        matches!(result, Err(NetworkError::Io(error)) if error.contains("rlpx_request_timeout:headers:"))
    );
    assert!(!has_read_test_session(chain_id, peer));
}
