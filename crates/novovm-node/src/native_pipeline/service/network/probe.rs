//! Local file operations borrow the existing NOVONET reactor. At most one
//! bounded read future; no socket, server, node task or network retry owner.
use super::*;
use crate::native_pipeline::consensus::native_lifecycle::probe::{
    ProbeCommand, ProbeRelease, COMMAND_BYTES, DOUBLE_RELAY_COMMAND_BYTES, RELAY_COMMAND_BYTES,
    RELEASE_COMMAND_BYTES,
};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

type Read = Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send>>;
struct Release {
    command: ProbeCommand,
    due: Instant,
    anchor: novovm_network::network_time::ClockAnchorV1,
}
pub(super) struct Control {
    path: PathBuf,
    read: Option<Read>,
    next: Instant,
    previous: Option<[u8; 32]>,
    accepted: u64,
    rejected: u64,
    last: Option<String>,
    release: Option<Release>,
}

impl Control {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path,
            read: None,
            next: Instant::now(),
            previous: None,
            accepted: 0,
            rejected: 0,
            last: None,
            release: None,
        }
    }
    pub(super) fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
        now: Instant,
    ) -> Option<Result<ProbeCommand>> {
        if self.release.is_some() {
            match wall_ms().and_then(|wall| self.take_release(now, wall)) {
                Ok(Some(command)) => return Some(Ok(command)),
                Err(error) => {
                    self.release = None;
                    return Some(Err(error));
                }
                Ok(None) => {}
            }
        }
        if self.read.is_none() && now >= self.next {
            self.next = now + Duration::from_secs(1);
            let path = self.path.clone();
            self.read = Some(Box::pin(async move {
                let metadata = tokio::fs::symlink_metadata(&path).await?;
                ensure!(
                    metadata.file_type().is_file()
                        && [
                            COMMAND_BYTES as u64,
                            RELAY_COMMAND_BYTES as u64,
                            DOUBLE_RELAY_COMMAND_BYTES as u64,
                            (RELAY_COMMAND_BYTES + 16) as u64,
                            RELEASE_COMMAND_BYTES as u64
                        ]
                        .contains(&metadata.len()),
                    "probe command must be one bounded regular file"
                );
                let mut bytes = Vec::with_capacity(RELEASE_COMMAND_BYTES + 1);
                tokio::fs::File::open(path)
                    .await?
                    .take((RELEASE_COMMAND_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
                    .await?;
                ensure!(
                    bytes.len() as u64 == metadata.len(),
                    "probe command changed during bounded read"
                );
                Ok(bytes)
            }));
        }
        let result = match self.read.as_mut()?.as_mut().poll(cx) {
            Poll::Pending => return None,
            Poll::Ready(result) => result,
        };
        self.read = None;
        match result {
            Ok(bytes) => match wall_ms() {
                Ok(wall) => self.consume(&bytes, now, wall),
                Err(error) => Some(Err(error)),
            },
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                None
            }
            Err(error) => Some(Err(error)),
        }
    }
    fn consume(&mut self, bytes: &[u8], now: Instant, wall: u64) -> Option<Result<ProbeCommand>> {
        let hash = Sha256::digest(bytes).into();
        if self.previous == Some(hash) {
            return None;
        }
        self.previous = Some(hash);
        // A newly consumed cancel/command supersedes the sole pending release.
        self.release = None;
        if bytes.starts_with(b"NVPROBR1") {
            return match ProbeRelease::decode(bytes)
                .and_then(|release| self.schedule(release, now, wall))
            {
                Ok(()) => None,
                Err(error) => Some(Err(error)),
            };
        }
        Some(ProbeCommand::decode(bytes))
    }
    fn schedule(&mut self, release: ProbeRelease, now: Instant, wall: u64) -> Result<()> {
        let ProbeCommand::RelaySend(probe, _) = &release.command else {
            anyhow::bail!("release requires original relay send");
        };
        probe.validate_time(wall)?;
        let delay = release
            .at_ms
            .checked_sub(wall)
            .context("release already late")?;
        ensure!(
            (100..=5000).contains(&delay) && release.at_ms < probe.expires_at_ms,
            "release lead outside bounded original validity"
        );
        self.release = Some(Release {
            command: release.command,
            due: now + Duration::from_millis(delay),
            anchor: novovm_network::network_time::ClockAnchorV1::from_parts(wall, now),
        });
        self.last = Some("scheduled_not_admitted_or_DONE".into());
        Ok(())
    }
    fn take_release(&mut self, now: Instant, wall: u64) -> Result<Option<ProbeCommand>> {
        let Some(release) = &self.release else {
            return Ok(None);
        };
        ensure!(
            release.anchor.at(now)?.abs_diff(wall) <= 50,
            "scheduled release clock jump"
        );
        if now < release.due {
            return Ok(None);
        }
        ensure!(
            now.duration_since(release.due) <= Duration::from_millis(20),
            "scheduled release missed deadline"
        );
        Ok(self.release.take().map(|release| release.command))
    }
    pub(super) fn record(&mut self, result: Result<[u8; 32]>) {
        match result {
            Ok(_) => {
                self.accepted = self.accepted.saturating_add(1);
                self.last = Some("accepted_by_original_owner_not_DONE".into());
            }
            Err(error) => {
                self.rejected = self.rejected.saturating_add(1);
                self.last = Some(error.to_string().chars().take(160).collect());
            }
        }
    }
    pub(super) fn report_snapshot(&self) -> impl Serialize + Send + use<> {
        use crate::native_pipeline::report::record;
        record! { enabled: true, accepted_commands: self.accepted, rejected_commands: self.rejected,
        last: self.last.clone(), release_pending: self.release.is_some(),
        scope: "local commands; never business or transport completion" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(at_ms: u64) -> ProbeRelease {
        ProbeRelease {
            command: ProbeCommand::RelaySend(
                NetworkProbe {
                    nonce: [1; 32],
                    sender: [2; 32],
                    recipient: [3; 32],
                    issued_at_ms: 1000,
                    expires_at_ms: 11000,
                },
                vec![[4; 32]],
            ),
            at_ms,
        }
    }
    #[test]
    fn scheduled_release_consumed_cancel_replacement_and_duplicate_do_not_rearm() -> Result<()> {
        let now = Instant::now();
        let mut control = Control::new(PathBuf::from("unused"));
        let scheduled = release(5000).encode()?;
        assert!(control.consume(&scheduled, now, 1000).is_none());
        assert!(control.release.is_some());
        assert!(control.consume(&scheduled, now, 1000).is_none());
        assert!(control
            .take_release(now + Duration::from_millis(4000), 5000)?
            .is_some());
        assert!(control
            .consume(&scheduled, now + Duration::from_millis(4001), 5001)
            .is_none());
        assert!(
            control.release.is_none(),
            "same bytes must not rearm after release"
        );
        control.previous = None;
        assert!(control.consume(&scheduled, now, 1000).is_none());
        let ProbeCommand::RelaySend(probe, via) = release(5000).command else {
            unreachable!()
        };
        let cancel = ProbeCommand::RelayCancel(probe, via).encode()?;
        assert!(matches!(
            control.consume(&cancel, now, 1000).unwrap()?,
            ProbeCommand::RelayCancel(..)
        ));
        assert!(control.release.is_none());
        assert!(control.consume(&scheduled, now, 1000).is_none());
        assert!(control
            .consume(b"not a command", now, 1000)
            .unwrap()
            .is_err());
        assert!(
            control.release.is_none(),
            "invalid replacement must not leave queued release"
        );
        Ok(())
    }
    #[test]
    fn scheduled_release_ignores_read_phase_and_fires_once_on_monotonic_due() -> Result<()> {
        let start = Instant::now();
        for read_delay in [100, 602, 1100] {
            let mut control = Control::new(PathBuf::from("unused"));
            let read_at = start + Duration::from_millis(read_delay);
            control.schedule(release(5000), read_at, 1000 + read_delay)?;
            // Next file read may be arbitrarily later; release bypasses that gate.
            control.next = start + Duration::from_secs(30);
            assert!(control
                .take_release(start + Duration::from_millis(3999), 4999)?
                .is_none());
            let command = control
                .take_release(start + Duration::from_millis(4000), 5000)?
                .unwrap();
            let ProbeCommand::RelaySend(probe, _) = command else {
                panic!("not original send")
            };
            assert_eq!((probe.issued_at_ms, probe.expires_at_ms), (1000, 11000));
            assert!(control
                .take_release(start + Duration::from_millis(4001), 5001)?
                .is_none());
        }
        Ok(())
    }
    #[test]
    fn scheduled_release_rejects_late_jump_reverse_and_unbounded_lead() -> Result<()> {
        let now = Instant::now();
        for at in [999, 1000, 1099, 6001, 11000, u64::MAX] {
            assert!(Control::new(PathBuf::from("unused"))
                .schedule(release(at), now, 1000)
                .is_err());
        }
        for (elapsed, wall) in [(4000, 6000), (4000, 4000), (4021, 5021)] {
            let mut control = Control::new(PathBuf::from("unused"));
            control.schedule(release(5000), now, 1000)?;
            assert!(control
                .take_release(now + Duration::from_millis(elapsed), wall)
                .is_err());
        }
        let mut control = Control::new(PathBuf::from("unused"));
        control.schedule(release(5000), now, 1000)?;
        assert!(control
            .take_release(now - Duration::from_millis(1), 1000)
            .is_err());
        Ok(())
    }
    use crate::native_pipeline::consensus::transport::NetworkProbe;

    #[test]
    fn original_reactor_reads_one_fixed_probe_command_without_replay() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("novonet-control-{:016x}", rand::random::<u64>()));
        std::fs::create_dir(&root)?;
        let path = root.join("command.nvprobe");
        let probe = NetworkProbe {
            nonce: [1; 32],
            sender: [2; 32],
            recipient: [3; 32],
            issued_at_ms: 1,
            expires_at_ms: 2,
        };
        std::fs::write(&path, ProbeCommand::Start(probe.clone()).encode()?)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let mut control = Control::new(path.clone());
            let now = Instant::now();
            let first = tokio::time::timeout(
                Duration::from_secs(3),
                poll_fn(|cx| control.poll(cx, now).map_or(Poll::Pending, Poll::Ready)),
            )
            .await??;
            ensure!(matches!(first, ProbeCommand::Start(_)));
            // A future read of identical bytes returns no second command.
            let replay = tokio::time::timeout(
                Duration::from_millis(50),
                poll_fn(|cx| {
                    control
                        .poll(cx, now + Duration::from_secs(1))
                        .map_or(Poll::Pending, Poll::Ready)
                }),
            )
            .await;
            ensure!(replay.is_err(), "unchanged command replayed");
            std::fs::write(&path, ProbeCommand::Cancel(probe.clone()).encode()?)?;
            let cancel = tokio::time::timeout(
                Duration::from_secs(3),
                poll_fn(|cx| {
                    control
                        .poll(cx, now + Duration::from_secs(2))
                        .map_or(Poll::Pending, Poll::Ready)
                }),
            )
            .await??;
            ensure!(matches!(cancel, ProbeCommand::Cancel(_)));
            std::fs::write(
                &path,
                ProbeCommand::RelayArm(probe.clone(), vec![[4; 32]]).encode()?,
            )?;
            let relay = tokio::time::timeout(
                Duration::from_secs(3),
                poll_fn(|cx| {
                    control
                        .poll(cx, now + Duration::from_secs(3))
                        .map_or(Poll::Pending, Poll::Ready)
                }),
            )
            .await??;
            ensure!(matches!(relay, ProbeCommand::RelayArm(_, via) if via == vec![[4; 32]]));
            std::fs::write(
                &path,
                ProbeCommand::RelayArm(probe, vec![[4; 32], [5; 32]]).encode()?,
            )?;
            let double = tokio::time::timeout(
                Duration::from_secs(3),
                poll_fn(|cx| {
                    control
                        .poll(cx, now + Duration::from_secs(4))
                        .map_or(Poll::Pending, Poll::Ready)
                }),
            )
            .await??;
            ensure!(
                matches!(double, ProbeCommand::RelayArm(_, via) if via == vec![[4; 32], [5; 32]])
            );
            std::fs::write(&path, b"arbitrary input")?;
            let invalid = tokio::time::timeout(
                Duration::from_secs(3),
                poll_fn(|cx| {
                    control
                        .poll(cx, now + Duration::from_secs(5))
                        .map_or(Poll::Pending, Poll::Ready)
                }),
            )
            .await?;
            ensure!(invalid.is_err());
            Ok::<_, anyhow::Error>(())
        })
    }
}
