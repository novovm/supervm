use anyhow::{bail, Context, Result};
use pqcrypto_dilithium::dilithium5;
use pqcrypto_traits::sign::{DetachedSignature as _, PublicKey as _, SecretKey as _};
use serde_json::json;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::Command;

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(raw: &str) -> Result<Vec<u8>> {
    let s = raw.trim();
    if s.is_empty() {
        bail!("hex string is empty");
    }
    if s.len() % 2 != 0 {
        bail!("hex string length must be even");
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let to_nibble = |c: u8| -> Result<u8> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => bail!("invalid hex character: {}", c as char),
        }
    };
    let mut i = 0usize;
    while i < bytes.len() {
        let hi = to_nibble(bytes[i])?;
        let lo = to_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn usage() -> String {
    [
        "mldsa87-vote-signer usage:",
        "  keygen",
        "  sign --message-hex <hex> --secret-env <ENV_NAME>",
        "  sign --message-hex <hex> --secret-file <PATH>",
        "  sign --message-hex <hex> --secret-stdin",
        "  sign --message-hex <hex> --provider-cmd <COMMAND_LINE>",
        "       (aliases: --kms-cmd / --hsm-cmd; receives MLDSA_MESSAGE_HEX env, returns signature_hex)",
        "  sign --message-hex <hex> --secret-hex <hex> --allow-insecure-secret-arg",
    ]
    .join("\n")
}

fn run_keygen() -> Result<()> {
    let (public_key, secret_key) = dilithium5::keypair();
    let value = json!({
        "pubkey_hex": encode_hex(public_key.as_bytes()),
        "secret_hex": encode_hex(secret_key.as_bytes()),
    });
    println!("{}", value);
    Ok(())
}

fn run_sign(args: &[String]) -> Result<()> {
    let mut message_hex = String::new();
    let mut secret_hex = String::new();
    let mut secret_env = String::new();
    let mut secret_file = String::new();
    let mut secret_stdin = false;
    let mut provider_cmd = String::new();
    let mut allow_insecure_secret_arg = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--message-hex" => {
                i += 1;
                if i >= args.len() {
                    bail!("missing value for --message-hex");
                }
                message_hex = args[i].clone();
            }
            "--secret-hex" => {
                i += 1;
                if i >= args.len() {
                    bail!("missing value for --secret-hex");
                }
                secret_hex = args[i].clone();
            }
            "--secret-env" => {
                i += 1;
                if i >= args.len() {
                    bail!("missing value for --secret-env");
                }
                secret_env = args[i].clone();
            }
            "--secret-file" => {
                i += 1;
                if i >= args.len() {
                    bail!("missing value for --secret-file");
                }
                secret_file = args[i].clone();
            }
            "--secret-stdin" => {
                secret_stdin = true;
            }
            "--provider-cmd" | "--kms-cmd" | "--hsm-cmd" => {
                i += 1;
                if i >= args.len() {
                    bail!("missing value for {}", args[i - 1]);
                }
                provider_cmd = args[i].clone();
            }
            "--allow-insecure-secret-arg" => {
                allow_insecure_secret_arg = true;
            }
            token => bail!("unknown argument: {}", token),
        }
        i += 1;
    }

    if message_hex.is_empty() {
        bail!("{}", usage());
    }
    let mut secret_sources = 0u32;
    if !secret_hex.is_empty() {
        secret_sources += 1;
    }
    if !secret_env.is_empty() {
        secret_sources += 1;
    }
    if !secret_file.is_empty() {
        secret_sources += 1;
    }
    if secret_stdin {
        secret_sources += 1;
    }
    if !provider_cmd.is_empty() {
        secret_sources += 1;
    }
    if secret_sources != 1 {
        bail!("use exactly one key source: --secret-env|--secret-file|--secret-stdin|--provider-cmd|--secret-hex");
    }
    if !secret_hex.is_empty() && !allow_insecure_secret_arg {
        bail!(
            "refusing insecure --secret-hex argument; use --secret-env or pass --allow-insecure-secret-arg explicitly"
        );
    }

    let message = decode_hex(&message_hex).context("decode message_hex failed")?;
    if !provider_cmd.is_empty() {
        let signature_hex = run_external_provider(provider_cmd.as_str(), message_hex.as_str())
            .context("provider-cmd sign failed")?;
        let signature_bytes = decode_hex(signature_hex.as_str()).context("decode signature failed")?;
        let _ = dilithium5::DetachedSignature::from_bytes(&signature_bytes)
            .map_err(|_| anyhow::anyhow!("invalid ML-DSA-87 signature bytes from provider"))?;
        let value = json!({
            "signature_hex": encode_hex(&signature_bytes),
        });
        println!("{}", value);
        return Ok(());
    }

    let secret_raw = if !secret_env.is_empty() {
        env::var(&secret_env)
            .with_context(|| format!("missing secret in env var {}", secret_env))?
    } else if !secret_file.is_empty() {
        fs::read_to_string(&secret_file)
            .with_context(|| format!("failed to read secret file {}", secret_file))?
            .trim()
            .to_string()
    } else if secret_stdin {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .context("failed to read secret from stdin")?;
        input.trim().to_string()
    } else {
        secret_hex
    };
    let secret_bytes = decode_hex(&secret_raw).context("decode secret failed")?;
    let secret_key = dilithium5::SecretKey::from_bytes(&secret_bytes)
        .map_err(|_| anyhow::anyhow!("invalid ML-DSA-87 secret key bytes"))?;
    let signature = dilithium5::detached_sign(&message, &secret_key);
    let value = json!({
        "signature_hex": encode_hex(signature.as_bytes()),
    });
    println!("{}", value);
    Ok(())
}

fn run_external_provider(command_line: &str, message_hex: &str) -> Result<String> {
    if command_line.trim().is_empty() {
        bail!("provider command line is empty");
    }
    let output = if cfg!(windows) {
        Command::new("cmd")
            .args(["/C", command_line])
            .env("MLDSA_MESSAGE_HEX", message_hex)
            .output()
            .context("failed to spawn provider command")?
    } else {
        Command::new("sh")
            .args(["-c", command_line])
            .env("MLDSA_MESSAGE_HEX", message_hex)
            .output()
            .context("failed to spawn provider command")?
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("provider command failed: {}", stderr.trim());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        bail!("provider command returned empty output");
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&stdout) {
        if let Some(sig) = value.get("signature_hex").and_then(|v| v.as_str()) {
            return Ok(sig.trim().to_string());
        }
    }
    Ok(stdout)
}

fn main() {
    if let Err(err) = real_main() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        bail!("{}", usage());
    }
    match args[0].as_str() {
        "keygen" => run_keygen(),
        "sign" => run_sign(&args[1..]),
        other => bail!("unknown command: {}\n{}", other, usage()),
    }
}
