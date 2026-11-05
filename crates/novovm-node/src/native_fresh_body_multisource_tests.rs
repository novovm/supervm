// Capture authenticated traffic once, then control only its delivery order to
// the original bounded inbox. The proof/body come from the real AOEM sequence.
fn exercise_body_multisource_recovery(
    proof: &crate::native_block_ledger::NovNativeFreshFinalityProofV1,
    raws: &[Vec<u8>],
) {
    use crate::native_block_seal::round_wire::{
        encode_nov_native_seal_round_wire_v1 as encode, is_nov_native_seal_round_wire_v1,
    };
    use crate::native_candidate_body::{
        network::{CandidateBodyInboxV1 as Inbox, CandidateBodySenderV1 as Sender},
        CandidateBodyAssemblerV1 as Assembler,
    };
    use crate::product_mainline_overlay::{
        ProductMainlineOverlayEventV1 as Event, ProductMainlineOverlayInboundV1 as Inbound,
    };
    use sha2::{Digest, Sha256};
    use std::time::{Duration, Instant};

    let authority = &proof.authority;
    let proposal = proof.witness.proposal().unwrap();
    let height = proposal.subject.height;
    let proposer = authority.transport_peer_id(proposal.proposer_id).unwrap();
    crate::native_block_seal::tests::native_seal_round_network::with_service_test_transports(
        authority.chain_id,
        |peers| {
            let source_b = peers
                .iter()
                .position(|(runtime, _)| runtime.startup().local_peer_id == proposer)
                .unwrap();
            let source_a = (0..peers.len()).find(|index| *index != source_b).unwrap();
            let target = (0..peers.len())
                .find(|index| *index != source_a && *index != source_b)
                .unwrap();
            let target_peer = &peers[target].0.startup().local_peer_id;
            let sources = [source_a, source_b];
            let mut frame_count = 0;
            let mut senders = sources
                .iter()
                .map(|index| {
                    let source = &peers[*index].0.startup().local_peer_id;
                    let wire = encode(&proof.witness, authority, height, source).unwrap();
                    frame_count = 1 + Assembler::new(&wire, authority, height, source)
                        .unwrap()
                        .encode_chunks(raws)
                        .unwrap()
                        .len();
                    Sender::new(wire, raws, authority, height, source, target_peer).unwrap()
                })
                .collect::<Vec<_>>();
            let mut captured: [Vec<Inbound>; 2] = [Vec::new(), Vec::new()];
            let started = Instant::now();
            while captured.iter().any(|frames| frames.len() < frame_count) {
                for (index, sender) in senders.iter_mut().enumerate() {
                    sender
                        .poll_at(peers[sources[index]].0, Instant::now())
                        .unwrap();
                }
                for (index, (runtime, _)) in peers.iter().enumerate() {
                    for event in runtime.drain_events(128) {
                        match event {
                            Event::Inbound(inbound) if index == target => {
                                let source = sources
                                    .iter()
                                    .position(|source| {
                                        peers[*source].0.startup().local_peer_id
                                            == inbound.source_peer_id
                                    })
                                    .unwrap();
                                if !captured[source]
                                    .iter()
                                    .any(|previous| previous.object_hash == inbound.object_hash)
                                {
                                    captured[source].push(inbound);
                                }
                            }
                            Event::WorkerFailed(error) => panic!("body multisource WSS: {error}"),
                            _ => (),
                        }
                    }
                }
                assert!(
                    started.elapsed() < Duration::from_secs(20),
                    "body multisource capture deadline"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            let manifests = captured.each_ref().map(|frames| {
                frames
                    .iter()
                    .find(|event| is_nov_native_seal_round_wire_v1(&event.frame.payload))
                    .unwrap()
            });
            let chunks = captured.each_ref().map(|frames| {
                frames
                    .iter()
                    .filter(|event| !is_nov_native_seal_round_wire_v1(&event.frame.payload))
                    .collect::<Vec<_>>()
            });
            let now = Instant::now();
            let mut inbox = Inbox::new(authority.clone(), height, now).unwrap();
            assert!(inbox.accept(manifests[0], now).unwrap().is_none());
            assert!(inbox.accept(manifests[1], now).unwrap().is_none());
            let mut complete = None;
            for chunk in &chunks[1] {
                complete = inbox.accept(chunk, now).expect(
                    "a certificate-only source must not block another authenticated body's fragments",
                ).or(complete);
            }
            assert_eq!(
                complete
                    .expect("second source must complete before the 30-second expiry")
                    .raw_txs,
                raws
            );
            // Delayed fragments/manifests must not deliver the proposal twice.
            for frames in &captured {
                for event in frames {
                    assert!(!matches!(inbox.accept(event, now), Ok(Some(_))));
                }
            }

            let mut isolated = Inbox::new(authority.clone(), height, now).unwrap();
            isolated.accept(manifests[0], now).unwrap();
            assert!(
                isolated.accept(chunks[1][0], now).is_err(),
                "a source needs its own authenticated manifest"
            );
            let mut bad = (*chunks[0][0]).clone();
            bad.frame.payload[40..44].copy_from_slice(&u32::MAX.to_be_bytes());
            let mut digest = Sha256::new();
            digest.update(b"novovm-candidate-body-chunk-object-v1\0");
            digest.update(&bad.frame.payload);
            bad.object_hash = digest.finalize().into();
            assert!(isolated.accept(&bad, now).is_err());
            isolated.accept(manifests[1], now).unwrap();
            let mut complete = None;
            for chunk in &chunks[1] {
                complete = isolated.accept(chunk, now).unwrap().or(complete);
            }
            assert_eq!(
                complete
                    .expect("failed source must not poison another source")
                    .raw_txs,
                raws
            );

            let mut expiring = Inbox::new(authority.clone(), height, now).unwrap();
            expiring.accept(manifests[0], now).unwrap();
            expiring
                .accept(manifests[1], now + Duration::from_secs(1))
                .unwrap();
            for manifest in manifests {
                expiring
                    .accept(manifest, now + Duration::from_secs(29))
                    .unwrap();
            }
            assert_eq!(expiring.expire(now + Duration::from_secs(30)).unwrap(), 1);
            assert_eq!(expiring.expire(now + Duration::from_secs(31)).unwrap(), 1);
        },
    );
}
