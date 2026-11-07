//! Local file operations borrow the existing NOVONET reactor. At most one
//! bounded read future; no socket, server, node task or network retry owner.
use super::*;
use crate::native_pipeline::consensus::native_lifecycle::probe::{ProbeCommand, COMMAND_BYTES};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

type Read = Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send>>;
pub(super) struct Control {
    path: PathBuf,
    read: Option<Read>,
    next: Instant,
    previous: Option<[u8; 32]>,
    accepted: u64,
    rejected: u64,
    last: Option<String>,
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
        }
    }
    pub(super) fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
        now: Instant,
    ) -> Option<Result<ProbeCommand>> {
        if self.read.is_none() && now >= self.next {
            self.next = now + Duration::from_secs(1);
            let path = self.path.clone();
            self.read = Some(Box::pin(async move {
                let metadata = tokio::fs::symlink_metadata(&path).await?;
                ensure!(
                    metadata.file_type().is_file() && metadata.len() == COMMAND_BYTES as u64,
                    "probe command must be one bounded regular file"
                );
                let mut bytes = Vec::with_capacity(COMMAND_BYTES + 1);
                tokio::fs::File::open(path)
                    .await?
                    .take((COMMAND_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
                    .await?;
                ensure!(
                    bytes.len() == COMMAND_BYTES,
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
            Ok(bytes) => {
                let hash = Sha256::digest(&bytes).into();
                if self.previous == Some(hash) {
                    return None;
                }
                self.previous = Some(hash);
                Some(ProbeCommand::decode(&bytes))
            }
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
    pub(super) fn observation(&self) -> serde_json::Value {
        serde_json::json!({"enabled":true,"accepted_commands":self.accepted,"rejected_commands":self.rejected,"last":self.last,"scope":"local commands; never business or transport completion"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            std::fs::write(&path, ProbeCommand::Cancel(probe).encode()?)?;
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
            std::fs::write(&path, b"arbitrary input")?;
            let invalid = tokio::time::timeout(
                Duration::from_secs(3),
                poll_fn(|cx| {
                    control
                        .poll(cx, now + Duration::from_secs(3))
                        .map_or(Poll::Pending, Poll::Ready)
                }),
            )
            .await?;
            ensure!(invalid.is_err());
            Ok::<_, anyhow::Error>(())
        })
    }
}
