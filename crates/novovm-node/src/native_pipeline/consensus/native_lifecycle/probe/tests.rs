//! Real original lifecycle/UDP/stores with branded component signing keys.
//! This is local correctness evidence, never physical four-host acceptance.
use super::*;
use crate::native_pipeline::consensus::tests::progress_integration::with_delivery_nodes_for_test;

fn warm_time(nodes: &mut [NativeLifecycle], pipelines: &[CandidatePipeline]) -> Result<()> {
    let start = Instant::now();
    loop {
        ensure!(
            start.elapsed() < Duration::from_secs(8),
            "authenticated time did not converge"
        );
        for (i, node) in nodes.iter_mut().enumerate() {
            node.poll_io(1000 + start.elapsed().as_millis() as u64, Instant::now())?;
            node.poll(&pipelines[i], Instant::now())?;
        }
        if nodes.iter().all(|node| {
            node.time_peers
                .iter()
                .all(|p| p.reference.interval(Instant::now()).is_ok())
        }) {
            return Ok(());
        }
        std::thread::yield_now();
    }
}

#[test]
fn probe_command_is_fixed_binary_and_rejects_arbitrary_content() -> Result<()> {
    let probe = NetworkProbe {
        nonce: [1; 32],
        sender: [2; 32],
        recipient: [3; 32],
        issued_at_ms: 1,
        expires_at_ms: 2,
    };
    for command in [
        ProbeCommand::Start(probe.clone()),
        ProbeCommand::Cancel(probe),
    ] {
        let bytes = command.encode()?;
        assert_eq!(bytes.len(), COMMAND_BYTES);
        assert_eq!(ProbeCommand::decode(&bytes)?.encode()?, bytes);
        for len in 0..bytes.len() {
            assert!(ProbeCommand::decode(&bytes[..len]).is_err());
        }
        let mut changed = bytes;
        changed[8] = 15;
        assert!(ProbeCommand::decode(&changed).is_err());
    }
    Ok(())
}

#[test]
#[ignore = "real AOEM and four original local UDP owners; not physical four-host acceptance"]
fn adaptive_time_original_owner_skew_jump_and_replay() -> Result<()> {
    with_delivery_nodes_for_test(|nodes, _, _, pipelines| {
        nodes[0].time_test_offset_ms = 900;
        nodes[1].time_test_offset_ms = -444;
        nodes[2].time_test_offset_ms = 1200;
        nodes[3].time_test_offset_ms = -900;
        warm_time(nodes, pipelines)?;
        let start = Instant::now();
        for node in nodes.iter_mut() {
            node.enable_network_probe(start);
        }
        let wall = nodes[0].time_clock()?.wall_ms();
        let probe = NetworkProbe {
            nonce: [92; 32],
            sender: nodes[0].progress.local_validator(),
            recipient: nodes[1].progress.local_validator(),
            issued_at_ms: wall,
            expires_at_ms: wall + 10000,
        };
        // Old direct comparison reproduces C's failure before the new path.
        assert!(probe
            .validate_time(nodes[1].time_clock()?.wall_ms())
            .is_err());
        let id = nodes[0].network_probe_command(ProbeCommand::Start(probe.clone()), wall, start)?;
        loop {
            ensure!(
                start.elapsed() < Duration::from_secs(8),
                "adaptive probe did not complete"
            );
            for (i, node) in nodes.iter_mut().enumerate() {
                node.poll_io(1000 + start.elapsed().as_millis() as u64, Instant::now())?;
                node.poll(&pipelines[i], Instant::now())?;
            }
            let sent = nodes[0]
                .probe
                .as_ref()
                .unwrap()
                .observations
                .iter()
                .any(|o| o.object_id == id && o.event == "probe_object_done");
            let received = nodes[1]
                .probe
                .as_ref()
                .unwrap()
                .observations
                .iter()
                .any(|o| o.object_id == id && o.event == "probe_received_integrity_verified");
            if sent && received {
                break;
            }
            std::thread::yield_now();
        }
        let peer = nodes[1]
            .peers
            .iter()
            .position(|key| *key == probe.sender)
            .unwrap();
        nodes[1].time_test_offset_ms += 2000;
        let new_probe = NetworkProbe {
            nonce: [93; 32],
            ..probe.clone()
        };
        nodes[1].receive_network_probe(peer, &new_probe, [93; 32], Instant::now());
        assert_eq!(
            nodes[1]
                .probe
                .as_ref()
                .unwrap()
                .observations
                .back()
                .unwrap()
                .event,
            "probe_rejected_time_untrusted_or_outside_validity"
        );
        warm_time(nodes, pipelines)?;
        nodes[1].receive_network_probe(peer, &new_probe, [93; 32], Instant::now());
        assert_eq!(
            nodes[1]
                .probe
                .as_ref()
                .unwrap()
                .observations
                .back()
                .unwrap()
                .event,
            "probe_rejected_duplicate"
        );
        for node in nodes {
            assert_eq!(node.executed(), 0);
            assert!(node.progress.head().is_none());
        }
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM stores and original native lifecycle; not real four-host acceptance"]
fn network_probe_original_owner_concurrent_done_and_isolation() -> Result<()> {
    with_delivery_nodes_for_test(|nodes, _, _, pipelines| {
        warm_time(nodes, pipelines)?;
        let start = Instant::now();
        let wall = wall_ms();
        for node in nodes.iter_mut() {
            #[cfg(feature = "iroh-transport")]
            assert!(node.network_probe_diagnostics().is_none());
            node.enable_network_probe(start);
            #[cfg(feature = "iroh-transport")]
            {
                let shared = node.network_probe_diagnostics().unwrap();
                node.enable_network_probe(start);
                assert!(std::sync::Arc::ptr_eq(
                    &shared,
                    &node.network_probe_diagnostics().unwrap()
                ));
            }
        }
        let make = |source: usize, target: usize, nonce| NetworkProbe {
            nonce: [nonce; 32],
            sender: nodes[source].progress.local_validator(),
            recipient: nodes[target].progress.local_validator(),
            issued_at_ms: wall - 100,
            expires_at_ms: wall + 9_900,
        };
        let first = make(0, 1, 17);
        let second = make(2, 3, 18);
        let object =
            nodes[0].network_probe_command(ProbeCommand::Start(first.clone()), wall, start)?;
        nodes[2].network_probe_command(ProbeCommand::Start(second), wall, start)?;
        assert!(nodes[0]
            .network_probe_command(ProbeCommand::Start(first.clone()), wall, start)
            .is_err());
        let deadline = start + Duration::from_secs(10);
        loop {
            ensure!(
                Instant::now() < deadline,
                "original probe did not reach both DONE and receiver acceptance"
            );
            for (i, node) in nodes.iter_mut().enumerate() {
                node.poll_io(1000 + start.elapsed().as_millis() as u64, Instant::now())?;
                node.poll(&pipelines[i], Instant::now())?;
                assert_eq!(node.executed(), 0);
                assert!(node.progress.head().is_none());
            }
            if [0, 2].iter().all(|i| {
                nodes[*i]
                    .probe
                    .as_ref()
                    .unwrap()
                    .observations
                    .iter()
                    .any(|o| o.event == "probe_object_done")
            }) && [1, 3].iter().all(|i| {
                nodes[*i]
                    .probe
                    .as_ref()
                    .unwrap()
                    .observations
                    .iter()
                    .any(|o| o.event == "probe_received_integrity_verified")
            }) {
                break;
            }
            std::thread::yield_now();
        }
        let incoming = nodes[1]
            .probe
            .as_ref()
            .unwrap()
            .observations
            .iter()
            .filter(|o| o.event == "probe_received_integrity_verified" && o.object_id == object)
            .collect::<Vec<_>>();
        assert_eq!(incoming.len(), 1);
        assert!(incoming[0].transport.session_id.is_some());
        let outgoing = nodes[0]
            .probe
            .as_ref()
            .unwrap()
            .observations
            .iter()
            .find(|o| o.event == "probe_object_done" && o.object_id == object)
            .unwrap();
        assert_eq!(
            outgoing.transport.session_id,
            incoming[0].transport.session_id
        );
        assert_eq!(outgoing.transport.path, "native_udp");
        assert!(matches!(
            outgoing.transport.route,
            Some(novovm_network::novorudp::carrier::NativeRoute::Udp(_))
        ));
        let evidence = nodes[0].channel.probe_observation().unwrap();
        assert!(evidence
            .objects
            .iter()
            .any(|o| o.transport_done_us.is_some()
                && o.acknowledged_best > 0
                && o.feedback_received.packets > 0));
        let error = nodes[0]
            .network_probe_command(
                ProbeCommand::Start(first.clone()),
                wall + 2000,
                start + Duration::from_secs(2),
            )
            .unwrap_err();
        assert!(error.to_string().contains("duplicate"));
        // Same authenticated source, but another declared target: no business
        // input and no accepted probe. Component wire DONE is not acceptance.
        let peer = nodes[1]
            .peers
            .iter()
            .position(|id| *id == first.sender)
            .unwrap();
        let mut wrong = first.clone();
        wrong.recipient = nodes[2].progress.local_validator();
        nodes[1].receive_network_probe(peer, &wrong, [42; 32], Instant::now());
        assert_eq!(
            nodes[1]
                .probe
                .as_ref()
                .unwrap()
                .observations
                .back()
                .unwrap()
                .event,
            "probe_rejected_receiver_or_sender"
        );
        nodes[1].receive_network_probe(peer, &first, object, Instant::now());
        assert_eq!(
            nodes[1]
                .probe
                .as_ref()
                .unwrap()
                .observations
                .back()
                .unwrap()
                .event,
            "probe_rejected_duplicate"
        );
        assert!(nodes.iter().all(|node| !node.is_frozen()));
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM stores and original native lifecycle; bounded cancellation and expiry"]
fn network_probe_original_owner_cancel_expiry_and_disabled() -> Result<()> {
    with_delivery_nodes_for_test(|nodes, _, _, _| {
        let start = Instant::now();
        let wall = wall_ms();
        let probe = NetworkProbe {
            nonce: [21; 32],
            sender: nodes[0].progress.local_validator(),
            recipient: nodes[1].progress.local_validator(),
            issued_at_ms: wall,
            expires_at_ms: wall + 10_000,
        };
        assert!(nodes[0]
            .network_probe_command(ProbeCommand::Start(probe.clone()), wall, start)
            .is_err());
        nodes[0].enable_network_probe(start);
        let count = nodes[0].pending_deliveries();
        nodes[0].network_probe_command(ProbeCommand::Start(probe.clone()), wall, start)?;
        nodes[0].network_probe_command(ProbeCommand::Cancel(probe.clone()), wall, start)?;
        assert_eq!(nodes[0].pending_deliveries(), count);
        assert_eq!(
            nodes[0]
                .probe
                .as_ref()
                .unwrap()
                .observations
                .back()
                .unwrap()
                .event,
            "probe_cancelled"
        );
        let next = NetworkProbe {
            nonce: [22; 32],
            issued_at_ms: wall + 2000,
            expires_at_ms: wall + 3000,
            ..probe.clone()
        };
        nodes[0].network_probe_command(
            ProbeCommand::Start(next),
            wall + 2000,
            start + Duration::from_secs(2),
        )?;
        nodes[0].expire_network_probe(start + Duration::from_secs(4));
        assert_eq!(nodes[0].pending_deliveries(), count);
        assert_eq!(
            nodes[0]
                .probe
                .as_ref()
                .unwrap()
                .observations
                .back()
                .unwrap()
                .event,
            "probe_expired_before_admission"
        );
        assert!(nodes[1].network_probe_observation().is_none());
        assert!(nodes
            .iter()
            .all(|node| node.executed() == 0 && node.progress.head().is_none()));
        Ok(())
    })
}

#[test]
#[ignore = "real AOEM stores/native lifecycle; admitted cancellation drains unchanged original sender"]
fn network_probe_original_owner_admitted_cancel_drains_and_receive_quota() -> Result<()> {
    with_delivery_nodes_for_test(|nodes, _, _, pipelines| {
        let start = Instant::now();
        let wall = wall_ms();
        for node in nodes.iter_mut() {
            node.enable_network_probe(start);
        }
        let probe = NetworkProbe {
            nonce: [31; 32],
            sender: nodes[0].progress.local_validator(),
            recipient: nodes[1].progress.local_validator(),
            issued_at_ms: wall,
            expires_at_ms: wall + 10_000,
        };
        let object =
            nodes[0].network_probe_command(ProbeCommand::Start(probe.clone()), wall, start)?;
        nodes[0].poll_io(1000, start)?;
        let peer = nodes[0]
            .peers
            .iter()
            .position(|id| *id == probe.recipient)
            .unwrap();
        assert!(nodes[0].deliveries[peer]
            .iter()
            .any(|d| d.prepared.fragment_id() == object && d.admitted));
        nodes[0].network_probe_command(ProbeCommand::Cancel(probe.clone()), wall, start)?;
        assert!(
            nodes[0]
                .probe
                .as_ref()
                .unwrap()
                .active
                .as_ref()
                .unwrap()
                .cancel_requested
        );
        let deadline = start + Duration::from_secs(10);
        while nodes[0].probe.as_ref().unwrap().active.is_some() {
            ensure!(
                Instant::now() < deadline,
                "admitted cancellation failed to drain"
            );
            for (index, node) in nodes.iter_mut().enumerate() {
                node.poll_io(1000 + start.elapsed().as_millis() as u64, Instant::now())?;
                node.poll(&pipelines[index], start)?;
                assert!(!node.is_frozen());
                assert_eq!(node.executed(), 0);
            }
        }
        assert_eq!(
            nodes[0]
                .probe
                .as_ref()
                .unwrap()
                .observations
                .back()
                .unwrap()
                .event,
            "probe_cancelled_drained_object_done"
        );
        // Replay/rate checks consume no additional transport or business queue.
        let remote_peer = nodes[1]
            .peers
            .iter()
            .position(|id| *id == probe.sender)
            .unwrap();
        while nodes[1].probe.as_ref().unwrap().receive_count < 4 {
            let nonce = 100 + nodes[1].probe.as_ref().unwrap().receive_count as u8;
            let next = NetworkProbe {
                nonce: [nonce; 32],
                ..probe.clone()
            };
            nodes[1].receive_network_probe(remote_peer, &next, [nonce; 32], start);
        }
        let next = NetworkProbe {
            nonce: [127; 32],
            ..probe
        };
        for _ in 0..128 {
            nodes[1].receive_network_probe(remote_peer, &next, [127; 32], start);
        }
        let state = nodes[1].probe.as_ref().unwrap();
        assert_eq!(state.observations.len(), HISTORY);
        assert!(state.evicted > 0 && state.received.len() <= 4);
        assert_eq!(
            state.observations.back().unwrap().event,
            "probe_rejected_rate_or_quota"
        );
        Ok(())
    })
}
