//! Local binary commands for the existing native owner. No sockets or keys.
use crate::{error::CtlError, output};
use clap::{Args, Subcommand};
use novovm_node::native_pipeline::consensus::{
    native_lifecycle::probe::{ProbeCommand, COMMAND_BYTES},
    transport::{probe::MAX_LIFETIME_MS, NetworkProbe},
};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Args)]
pub struct NativeNetworkProbeArgs {
    #[command(subcommand)]
    pub command: Command,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Publish a fixed non-business command; original owner still enforces all quotas.
    Prepare {
        #[arg(long)]
        sender: String,
        #[arg(long)]
        recipient: String,
        #[arg(long)]
        nonce: Option<String>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Cancel only the exact original diagnostic command, never a business send.
    Cancel {
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}
fn hash(value: &str) -> Result<[u8; 32], CtlError> {
    if value.len() != 64 {
        return Err(CtlError::InvalidArgument("expected 32-byte hex".into()));
    }
    let mut out = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        out[index] = u8::from_str_radix(
            std::str::from_utf8(pair).map_err(|e| CtlError::InvalidArgument(e.to_string()))?,
            16,
        )
        .map_err(|e| CtlError::InvalidArgument(e.to_string()))?;
    }
    Ok(out)
}
fn read(path: &Path) -> Result<ProbeCommand, CtlError> {
    let meta =
        std::fs::symlink_metadata(path).map_err(|e| CtlError::FileReadFailed(e.to_string()))?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != COMMAND_BYTES as u64 {
        return Err(CtlError::InvalidArgument(
            "not a bounded probe command file".into(),
        ));
    }
    let mut bytes = Vec::with_capacity(COMMAND_BYTES + 1);
    std::fs::File::open(path)
        .and_then(|file| {
            file.take((COMMAND_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
        })
        .map_err(|e| CtlError::FileReadFailed(e.to_string()))?;
    ProbeCommand::decode(&bytes).map_err(|e| CtlError::InvalidArgument(e.to_string()))
}
fn publish(path: &Path, command: &ProbeCommand) -> Result<(), CtlError> {
    if !path.is_absolute() || path.extension().and_then(|s| s.to_str()) != Some("nvprobe") {
        return Err(CtlError::InvalidArgument(
            "absolute .nvprobe command path required".into(),
        ));
    }
    match std::fs::symlink_metadata(path) {
        Ok(_) => {
            read(path)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(CtlError::FileReadFailed(e.to_string())),
    }
    let bytes = command
        .encode()
        .map_err(|e| CtlError::InvalidArgument(e.to_string()))?;
    let stage = path.with_extension(format!("nvprobe.{}.pending", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage)
        .map_err(|e| CtlError::FileWriteFailed(e.to_string()))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| CtlError::FileWriteFailed(e.to_string()))?;
    drop(file);
    std::fs::rename(&stage, path).map_err(|e| CtlError::FileWriteFailed(e.to_string()))
}
pub fn run(args: NativeNetworkProbeArgs) -> Result<(), CtlError> {
    let result = (|| {
        let (command, path) = match args.command {
            Command::Prepare {
                sender,
                recipient,
                nonce,
                output,
            } => {
                let sender = hash(&sender)?;
                let recipient = hash(&recipient)?;
                let time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| CtlError::InvalidArgument(e.to_string()))?;
                let nonce = match nonce {
                    Some(nonce) => hash(&nonce)?,
                    None => {
                        let mut h = Sha256::new();
                        h.update(b"NOVONET/local-probe-nonce/v1");
                        h.update(time.as_nanos().to_be_bytes());
                        h.update(std::process::id().to_be_bytes());
                        h.update(sender);
                        h.update(recipient);
                        h.finalize().into()
                    }
                };
                let issued = u64::try_from(time.as_millis())
                    .map_err(|e| CtlError::InvalidArgument(e.to_string()))?;
                let expires = issued
                    .checked_add(MAX_LIFETIME_MS)
                    .ok_or_else(|| CtlError::InvalidArgument("clock overflow".into()))?;
                (
                    ProbeCommand::Start(NetworkProbe {
                        nonce,
                        sender,
                        recipient,
                        issued_at_ms: issued,
                        expires_at_ms: expires,
                    }),
                    output,
                )
            }
            Command::Cancel { from, output } => {
                let probe = match read(&from)? {
                    ProbeCommand::Start(p) | ProbeCommand::Cancel(p) => p,
                };
                (ProbeCommand::Cancel(probe), output)
            }
        };
        publish(&path, &command)?;
        let probe = match &command {
            ProbeCommand::Start(p) | ProbeCommand::Cancel(p) => p,
        };
        output::print_success_json(
            "native-network-probe",
            &serde_json::json!({
                "scope":"local_binary_command_only","path":path,"bytes":COMMAND_BYTES,
                "nonce":probe.nonce,"sender_validator_id":probe.sender,"recipient_validator_id":probe.recipient,
                "issued_at_ms":probe.issued_at_ms,"expires_at_ms":probe.expires_at_ms,
                "sent":false,"object_done":false,"business_receipt":false,"node_started":false
            }),
        )
    })();
    if let Err(error) = &result {
        output::print_error_json("native-network-probe", error);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_probe_publish_refuses_other_files_and_reuses_only_valid_commands(
    ) -> Result<(), CtlError> {
        let root = std::env::temp_dir().join(format!(
            "novovm-probe-cli-{}-{}",
            std::process::id(),
            output::now_unix_ms()
        ));
        std::fs::create_dir(&root).map_err(|e| CtlError::FileWriteFailed(e.to_string()))?;
        let path = root.join("command.nvprobe");
        let probe = NetworkProbe {
            nonce: [1; 32],
            sender: [2; 32],
            recipient: [3; 32],
            issued_at_ms: 1,
            expires_at_ms: 2,
        };
        publish(&path, &ProbeCommand::Start(probe.clone()))?;
        assert!(matches!(read(&path)?, ProbeCommand::Start(_)));
        publish(&path, &ProbeCommand::Cancel(probe))?;
        assert!(matches!(read(&path)?, ProbeCommand::Cancel(_)));
        let sentinel = root.join("untouched.nvprobe");
        std::fs::write(&sentinel, b"not a probe command")
            .map_err(|e| CtlError::FileWriteFailed(e.to_string()))?;
        assert!(publish(&sentinel, &read(&path)?).is_err());
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"not a probe command");
        assert!(publish(&root.join("key.protected"), &read(&path)?).is_err());
        assert!(!root.join("key.protected").exists());
        assert!(ProbeCommand::decode(b"{\"submit\":\"asset\"}").is_err());
        Ok(())
    }
}
