//! Two fixed lanes within the existing owner, not two execution authorities.
//! Each native context is opened, used and dropped on its own thread. The CPU
//! session and integer context already existed; no native context is added.
//! One shared queue budget and the original pipeline permits bound both lanes.
use super::*;
use std::sync::atomic::AtomicUsize;

pub(super) struct Budget {
    queued: AtomicUsize,
    limit: usize,
}
impl Budget {
    pub(super) fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            queued: AtomicUsize::new(0),
            limit,
        })
    }
    fn acquire(self: &Arc<Self>) -> Option<Slot> {
        self.queued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < self.limit).then_some(n + 1)
            })
            .ok()
            .map(|_| Slot(Arc::clone(self)))
    }
}
struct Slot(Arc<Budget>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.queued.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(super) struct Queued {
    command: Command,
    slot: Slot,
}
impl Queued {
    pub(super) fn into_command(self) -> Command {
        let Self { command, slot } = self;
        drop(slot); // Queue permit only; pipeline retains end-to-end permits.
        command
    }
}
pub(super) struct Sender {
    sender: mpsc::SyncSender<Queued>,
    budget: Arc<Budget>,
    failed: Arc<AtomicBool>,
}
impl Sender {
    pub(super) fn try_send(&self, command: Command) -> Result<(), mpsc::TrySendError<Command>> {
        if self.failed.load(Ordering::Acquire) {
            return Err(mpsc::TrySendError::Disconnected(command));
        }
        let Some(slot) = self.budget.acquire() else {
            return Err(mpsc::TrySendError::Full(command));
        };
        self.sender
            .try_send(Queued { command, slot })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(queued) => mpsc::TrySendError::Full(queued.into_command()),
                mpsc::TrySendError::Disconnected(queued) => {
                    mpsc::TrySendError::Disconnected(queued.into_command())
                }
            })
    }
}
pub(super) fn channel(
    budget: Arc<Budget>,
    failed: Arc<AtomicBool>,
) -> (Sender, mpsc::Receiver<Queued>) {
    let (sender, receiver) = mpsc::sync_channel(budget.limit);
    (
        Sender {
            sender,
            budget,
            failed,
        },
        receiver,
    )
}

pub(super) struct UnwindLatch(pub(super) Arc<AtomicBool>);
impl Drop for UnwindLatch {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0.store(true, Ordering::Release);
        }
    }
}
pub(super) fn checked<T>(failed: &AtomicBool, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    ensure!(
        !failed.load(Ordering::Acquire),
        "compute owner previously panicked; explicit recovery required"
    );
    let mut panicked = false;
    let result = run_checked(&mut panicked, operation);
    if panicked {
        failed.store(true, Ordering::Release);
    }
    // A peer lane may have failed while this work ran. Do not expose its result
    // as usable work from a healthy owner; never reopen either session here.
    ensure!(
        !failed.load(Ordering::Acquire),
        "compute owner panicked; explicit recovery required"
    );
    result
}

pub(super) struct IntegerLane {
    pub(super) sender: Sender,
    worker: JoinHandle<()>,
}
impl IntegerLane {
    pub(super) fn start(
        config: Arc<ComputeConfig>,
        budget: Arc<Budget>,
        failed: Arc<AtomicBool>,
        notify: Option<Thread>,
    ) -> Result<Self> {
        let (sender, receiver) = channel(budget, Arc::clone(&failed));
        let (ready, ready_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("novovm-compute-integer".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                let notification = Notification(notify);
                let _fault = UnwindLatch(Arc::clone(&failed));
                let resources = ResidentTransferResources::new(
                    &config.library,
                    config.workers,
                    config.plan.transactions,
                );
                match resources {
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                    Ok(mut resources) => {
                        if ready.send(Ok(())).is_err() {
                            return;
                        }
                        while let Ok(queued) = receiver.recv() {
                            match queued.into_command() {
                                Command::Execute {
                                    queued,
                                    input,
                                    reply,
                                } => {
                                    profile::elapsed(profile::Phase::QueueCandidate, queued);
                                    let result = checked(&failed, || {
                                        domain_matches(config.domain, input.plan().context())?;
                                        let input = {
                                            let _span =
                                                profile::span(profile::Phase::FinalizeCapture);
                                            input.finalize_capture()?
                                        };
                                        let executed =
                                            input.execute_integer_plan(&mut resources)?;
                                        packet(&config, executed)
                                    });
                                    let _ = reply.send(result);
                                }
                                #[cfg(test)]
                                Command::Hold { entered, release } => {
                                    if entered.send(()).is_ok() {
                                        let _ = release.recv();
                                    }
                                }
                                _ => panic!("non-execution command routed to integer lane"),
                            }
                            notification.wake();
                        }
                    }
                }
            })
            .context("start resident integer lane")?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { sender, worker }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error.context("initialize resident integer lane"))
            }
            Err(error) => {
                let _ = worker.join();
                Err(error).context("integer lane failed during initialization")
            }
        }
    }
    pub(super) fn shutdown(self) -> Result<()> {
        drop(self.sender);
        self.worker
            .join()
            .map_err(|_| anyhow::anyhow!("integer lane panicked"))
    }
}

#[cfg(test)]
mod tests;
