use super::*;

fn challenge(
    receiver: &mut Receiver,
    source: SocketAddr,
    sender: &mut Sender,
    bytes: &[u8],
    now: Instant,
) -> Result<Vec<u8>> {
    let init = sender.datagram(bytes)?;
    assert_eq!(init.len(), HEADER);
    assert!(sender.sent());
    let Received::Reply(reply) = receiver.receive(source, &init, now)? else {
        panic!("no cookie")
    };
    assert_eq!(reply.len(), init.len());
    sender.challenge(&reply)?;
    Ok(reply)
}

fn parts(sender: &mut Sender, bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut result = Vec::new();
    loop {
        let part = sender.datagram(bytes)?;
        assert!(part.len() <= MAX_DATAGRAM_BYTES);
        result.push(part);
        if sender.sent() {
            break;
        }
    }
    Ok(result)
}

#[test]
fn maximum_sized_transcript_reassembles_out_of_order_with_exact_duplicates() -> Result<()> {
    let now = Instant::now();
    let source = "127.0.0.1:1000".parse()?;
    let bytes = vec![7; MAX_TRANSCRIPT_BYTES];
    assert_eq!(bytes.len(), 5591);
    let mut receiver = Receiver::new(now);
    let mut sender = Sender::new(&bytes)?;
    challenge(&mut receiver, source, &mut sender, &bytes, now)?;
    assert!(receiver.pending.is_empty(), "Init allocated reassembly");
    let mut flight = parts(&mut sender, &bytes)?;
    assert_eq!(flight.len(), 6);
    flight.reverse();
    for part in &flight[..5] {
        assert!(matches!(
            receiver.receive(source, part, now)?,
            Received::Pending
        ));
        assert!(matches!(
            receiver.receive(source, part, now)?,
            Received::Pending
        ));
    }
    let Received::Complete(actual) = receiver.receive(source, &flight[5], now)? else {
        panic!("not complete")
    };
    assert_eq!(actual, bytes);
    assert!(receiver.pending.is_empty());
    Ok(())
}

#[test]
fn cookie_binds_source_owner_transfer_digest_length_and_expiry_before_allocation() -> Result<()> {
    let now = Instant::now();
    let source = "127.0.0.1:1000".parse()?;
    let bytes = vec![9; MAX_TRANSCRIPT_BYTES];
    let mut receiver = Receiver::new(now);
    let mut sender = Sender::new(&bytes)?;
    let reply = challenge(&mut receiver, source, &mut sender, &bytes, now)?;
    let part = sender.datagram(&bytes)?;
    for address in ["127.0.0.1:1001", "127.0.0.2:1000", "[::1]:1000"] {
        assert!(receiver.receive(address.parse()?, &part, now).is_err());
    }
    assert!(Receiver::new(now).receive(source, &part, now).is_err());
    for offset in [9, 25, 58, 59, 67] {
        let mut forged = part.clone();
        forged[offset] ^= 1;
        assert!(receiver.receive(source, &forged, now).is_err());
    }
    let mut other_attempt = Sender::new(&bytes)?;
    assert!(other_attempt.challenge(&reply).is_err());
    assert!(receiver.pending.is_empty());
    assert!(receiver.receive(source, &part, now + LIFETIME).is_err());
    Ok(())
}

#[test]
fn malformed_init_and_parts_never_allocate_or_reflect() -> Result<()> {
    let now = Instant::now();
    let source = "127.0.0.1:1000".parse()?;
    let bytes = vec![2; MAX_TRANSCRIPT_BYTES];
    let mut sender = Sender::new(&bytes)?;
    let init = sender.datagram(&bytes)?;
    let mut receiver = Receiver::new(now);
    for length in 0..init.len() {
        assert!(receiver.receive(source, &init[..length], now).is_err());
    }
    let mut extra = init.clone();
    extra.push(0);
    assert!(receiver.receive(source, &extra, now).is_err());
    for length in [0u16, (MAX_TRANSCRIPT_BYTES + 1) as u16, u16::MAX] {
        let mut wire = init.clone();
        wire[57..59].copy_from_slice(&length.to_be_bytes());
        assert!(receiver.receive(source, &wire, now).is_err());
    }
    challenge(&mut receiver, source, &mut sender, &bytes, now)?;
    let mut part = sender.datagram(&bytes)?;
    part[HEADER] = 6;
    assert!(receiver.receive(source, &part, now).is_err());
    part[HEADER] = 0;
    part.pop();
    assert!(receiver.receive(source, &part, now).is_err());
    assert!(receiver.pending.is_empty());
    assert!(Sender::new(&vec![0; MAX_TRANSCRIPT_BYTES + 1]).is_err());
    Ok(())
}

#[test]
fn missing_conflicting_and_tampered_parts_do_not_authenticate_or_extend_deadline() -> Result<()> {
    let now = Instant::now();
    let source = "127.0.0.1:1000".parse()?;
    let bytes = vec![3; MAX_TRANSCRIPT_BYTES];
    let mut receiver = Receiver::new(now);
    let mut sender = Sender::new(&bytes)?;
    let reply = challenge(&mut receiver, source, &mut sender, &bytes, now)?;
    let flight = parts(&mut sender, &bytes)?;
    receiver.receive(source, &flight[0], now)?;
    let deadline = receiver.pending[&source].deadline;
    let mut conflict = flight[0].clone();
    conflict[HEADER + 1] ^= 1;
    assert!(receiver.receive(source, &conflict, now).is_err());
    let later = now + Duration::from_secs(5);
    let retry_reply = challenge(&mut receiver, source, &mut sender, &bytes, later)?;
    assert_eq!(
        retry_reply, reply,
        "retry changed cookie/deadline for partial flight"
    );
    assert_eq!(receiver.pending[&source].deadline, deadline);
    assert!(matches!(
        receiver.receive(source, &flight[0], later)?,
        Received::Pending
    ));
    // A corrupt new part can be stored, but the completed digest must reject it.
    for (index, part) in flight.iter().enumerate().skip(1) {
        let mut wire = part.clone();
        if index == 1 {
            wire[HEADER + 1] ^= 1;
        }
        let outcome = receiver.receive(source, &wire, later);
        if index == flight.len() - 1 {
            assert!(outcome.is_err());
        } else {
            assert!(matches!(outcome?, Received::Pending));
        }
    }
    assert!(receiver.pending.is_empty());
    receiver.receive(source, &flight[0], later)?;
    receiver.expire(now + LIFETIME);
    assert!(receiver.pending.is_empty());
    Ok(())
}

#[test]
fn per_source_and_global_memory_are_bounded_without_challenging_peer_displacement() -> Result<()> {
    let now = Instant::now();
    let bytes = vec![8; MAX_TRANSCRIPT_BYTES];
    let mut receiver = Receiver::new(now);
    for index in 0..=MAX_ASSEMBLIES {
        let source = SocketAddr::from(([127, 0, 0, 1], 1000 + index as u16));
        let mut sender = Sender::new(&bytes)?;
        challenge(&mut receiver, source, &mut sender, &bytes, now)?;
        let result = receiver.receive(source, &sender.datagram(&bytes)?, now);
        if index == MAX_ASSEMBLIES {
            assert!(result.is_err());
        } else {
            result?;
        }
    }
    assert_eq!(receiver.pending.len(), MAX_ASSEMBLIES);
    assert_eq!(
        receiver
            .pending
            .values()
            .map(|v| v.bytes.len())
            .sum::<usize>(),
        178912
    );
    let mut replacement = Sender::new(&bytes)?;
    assert!(receiver
        .receive(
            "127.0.0.1:1000".parse()?,
            &replacement.datagram(&bytes)?,
            now
        )
        .is_err());
    receiver.expire(now + LIFETIME);
    challenge(
        &mut receiver,
        "127.0.0.1:1000".parse()?,
        &mut replacement,
        &bytes,
        now + LIFETIME,
    )?;
    assert!(receiver.pending.is_empty());
    Ok(())
}

#[test]
fn duplicate_challenge_does_not_rewind_flight_or_change_transcript() -> Result<()> {
    let now = Instant::now();
    let bytes = vec![8; MAX_TRANSCRIPT_BYTES];
    let mut receiver = Receiver::new(now);
    let mut sender = Sender::new(&bytes)?;
    let reply = challenge(
        &mut receiver,
        "127.0.0.1:1000".parse()?,
        &mut sender,
        &bytes,
        now,
    )?;
    let first = sender.datagram(&bytes)?;
    assert_eq!(
        first,
        sender.datagram(&bytes)?,
        "unaccepted send advanced cursor"
    );
    assert!(!sender.sent());
    let second = sender.datagram(&bytes)?;
    sender.challenge(&reply)?;
    assert_eq!(second, sender.datagram(&bytes)?);
    assert_ne!(first, second);
    assert!(sender.datagram(&bytes[..100]).is_err());
    while !sender.sent() {}
    assert!(
        !sender.challenge(&reply)?,
        "old challenge skipped retry and next Init"
    );
    assert_eq!(sender.datagram(&bytes)?[8], INIT);
    Ok(())
}

#[test]
fn completed_cookie_tail_cannot_block_new_flight_or_repeat_verification() -> Result<()> {
    let now = Instant::now();
    let source = "127.0.0.1:1000".parse()?;
    let bytes = vec![5; MAX_TRANSCRIPT_BYTES];
    let mut receiver = Receiver::new(now);
    let mut sender = Sender::new(&bytes)?;
    challenge(&mut receiver, source, &mut sender, &bytes, now)?;
    let flight = parts(&mut sender, &bytes)?;
    for part in &flight {
        receiver.receive(source, part, now)?;
    }
    assert!(receiver.pending.is_empty());
    for part in &flight {
        assert!(matches!(
            receiver.receive(source, part, now)?,
            Received::Pending
        ));
        assert!(receiver.pending.is_empty());
    }
    let mut fresh = Sender::new(&bytes)?;
    challenge(&mut receiver, source, &mut fresh, &bytes, now)?;
    receiver.receive(source, &fresh.datagram(&bytes)?, now)?;
    assert_eq!(receiver.pending.len(), 1);
    receiver.receive(source, &flight[5], now)?;
    assert_eq!(
        receiver.pending[&source].commitment.transfer,
        fresh.commitment.transfer
    );
    receiver.expire(now + LIFETIME);
    assert!(receiver.pending.is_empty() && receiver.completed.is_empty());
    Ok(())
}
