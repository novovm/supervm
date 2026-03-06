use anyhow::{bail, Context, Result};
use pqcrypto_dilithium::dilithium5;
use pqcrypto_traits::sign::{DetachedSignature as _, PublicKey as _, SecretKey as _};
use serde_json::json;
use std::env;

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
        "  sign --message-hex <hex> --secret-hex <hex>",
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
            token => bail!("unknown argument: {}", token),
        }
        i += 1;
    }

    if message_hex.is_empty() || secret_hex.is_empty() {
        bail!("{}", usage());
    }

    let message = decode_hex(&message_hex).context("decode message_hex failed")?;
    let secret_bytes = decode_hex(&secret_hex).context("decode secret_hex failed")?;
    let secret_key = dilithium5::SecretKey::from_bytes(&secret_bytes)
        .map_err(|_| anyhow::anyhow!("invalid ML-DSA-87 secret key bytes"))?;
    let signature = dilithium5::detached_sign(&message, &secret_key);
    let value = json!({
        "signature_hex": encode_hex(signature.as_bytes()),
    });
    println!("{}", value);
    Ok(())
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
