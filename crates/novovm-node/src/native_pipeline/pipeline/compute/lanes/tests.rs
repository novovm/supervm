use super::*;

fn hold() -> Command {
    Command::Hold {
        entered: mpsc::channel().0,
        release: mpsc::channel().1,
    }
}

#[test]
fn single_channel_is_bounded_without_shared_permits_and_keeps_failure_latch() {
    let failed = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = single_channel(1, Arc::clone(&failed));
    assert!(sender.budget.is_none());
    assert!(sender.try_send(hold()).is_ok());
    assert!(matches!(
        sender.try_send(hold()),
        Err(mpsc::TrySendError::Full(_))
    ));
    let queued = receiver.recv().unwrap();
    assert!(queued.slot.is_none());
    drop(queued.into_command());
    assert!(sender.try_send(hold()).is_ok());
    // Already accepted work remains owned, but no new work enters a failed owner.
    failed.store(true, Ordering::Release);
    assert!(matches!(
        sender.try_send(hold()),
        Err(mpsc::TrySendError::Disconnected(_))
    ));
    assert!(receiver.recv().unwrap().slot.is_none());
    failed.store(false, Ordering::Release); // Test fixture only, not runtime recovery.
    drop(receiver);
    assert!(matches!(
        sender.try_send(hold()),
        Err(mpsc::TrySendError::Disconnected(_))
    ));
}

#[test]
fn both_lanes_share_queue_cap_and_release_on_consume_or_disconnect() {
    let budget = Budget::new(2);
    let failed = Arc::new(AtomicBool::new(false));
    let (a, ar) = channel(Arc::clone(&budget), Arc::clone(&failed));
    let (b, br) = channel(Arc::clone(&budget), failed);
    assert!(a.try_send(hold()).is_ok());
    assert!(b.try_send(hold()).is_ok());
    assert!(matches!(
        a.try_send(hold()),
        Err(mpsc::TrySendError::Full(_))
    ));
    assert!(matches!(
        b.try_send(hold()),
        Err(mpsc::TrySendError::Full(_))
    ));
    drop(ar.recv().unwrap().into_command());
    assert_eq!(budget.queued.load(Ordering::Acquire), 1);
    assert!(b.try_send(hold()).is_ok());
    drop(br); // Both queued allocations and their permits are released.
    assert_eq!(budget.queued.load(Ordering::Acquire), 0);
    assert!(matches!(
        b.try_send(hold()),
        Err(mpsc::TrySendError::Disconnected(_))
    ));
    assert_eq!(budget.queued.load(Ordering::Acquire), 0);
}

#[test]
fn peer_unwind_latches_all_work_and_discards_racing_success() {
    let failed = Arc::new(AtomicBool::new(false));
    assert!(checked::<()>(&failed, || anyhow::bail!("ordinary rejection")).is_err());
    assert!(!failed.load(Ordering::Acquire));
    let (entered, entry) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let peer = Arc::clone(&failed);
    let worker = thread::spawn(move || {
        checked(&peer, || {
            entered.send(()).unwrap();
            held.recv().unwrap();
            Ok(123)
        })
    });
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(checked::<()>(&failed, || panic!("injected lane unwind")).is_err());
    release.send(()).unwrap();
    assert!(worker.join().unwrap().is_err());
    let ran = AtomicBool::new(false);
    assert!(checked(&failed, || {
        ran.store(true, Ordering::Relaxed);
        Ok(())
    })
    .is_err());
    assert!(!ran.load(Ordering::Relaxed));
    let (sender, _) = channel(Budget::new(1), failed);
    assert!(matches!(
        sender.try_send(hold()),
        Err(mpsc::TrySendError::Disconnected(_))
    ));
}

#[test]
fn outer_lane_unwind_is_latched_but_normal_exit_is_not() {
    let failed = Arc::new(AtomicBool::new(false));
    drop(UnwindLatch(Arc::clone(&failed)));
    assert!(!failed.load(Ordering::Acquire));
    let peer = Arc::clone(&failed);
    assert!(thread::spawn(move || {
        let _guard = UnwindLatch(peer);
        panic!("outer unwind");
    })
    .join()
    .is_err());
    assert!(failed.load(Ordering::Acquire));
}
