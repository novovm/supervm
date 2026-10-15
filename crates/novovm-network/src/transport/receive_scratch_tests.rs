use super::*;

#[test]
fn receive_scratch_keeps_exact_payloads_and_original_drain_bounds() {
    for drain_first in [false, true] {
        let mut receiver = UdpTransport::bind_for_chain(
            NodeId(78_600 + u64::from(drain_first)),
            "127.0.0.1:0",
            78_600 + u64::from(drain_first),
        )
        .unwrap();
        receiver.drain_first_enabled = drain_first;
        receiver.drain_batch_max = 2;
        receiver.drain_queue_limit = 2;
        let scratch_address = receiver.recv_buf.lock().unwrap().as_ptr() as usize;
        let scratch_capacity = receiver.recv_buf.lock().unwrap().len();
        for _ in 0..16 {
            assert!(receiver.receive_packet_v1().unwrap().is_none());
            let scratch = receiver.recv_buf.lock().unwrap();
            assert_eq!(scratch.as_ptr() as usize, scratch_address);
            assert_eq!(scratch.len(), scratch_capacity);
        }
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let originals = [vec![0xa7; 4096], vec![0x31; 7], Vec::new()];
        for packet in &originals {
            sender
                .send_to(packet, receiver.local_addr().unwrap())
                .unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        for original in originals {
            let (received, source) = loop {
                assert!(Instant::now() < deadline, "loopback receive deadline");
                if let Some(packet) = receiver.receive_packet_v1().unwrap() {
                    break packet;
                }
                std::thread::yield_now();
            };
            assert_eq!(source, sender.local_addr().unwrap());
            assert_eq!(received, original, "old scratch tail escaped");
            assert_eq!(received.capacity(), received.len());
            let queue = receiver.drain_queue.lock().unwrap();
            assert!(queue.len() <= receiver.drain_queue_limit.saturating_sub(1));
            assert!(queue
                .iter()
                .all(|(bytes, _)| bytes.capacity() == bytes.len()));
        }
        assert!(receiver.receive_packet_v1().unwrap().is_none());
        let scratch = receiver.recv_buf.lock().unwrap();
        assert_eq!(scratch.as_ptr() as usize, scratch_address);
        assert_eq!(scratch.len(), scratch_capacity);
        // A shorter/empty packet did not require clearing the unused tail.
        assert_eq!(scratch[4095], 0xa7);
        assert!(receiver.peers.is_empty());
        assert!(receiver.source_pins.is_empty());
    }
}
