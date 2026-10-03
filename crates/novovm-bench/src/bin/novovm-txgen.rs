// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology

#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_protocol::{encode_local_tx_wire_v1, LocalTxWireV1, LOCAL_TX_WIRE_V1_CODEC};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

const LOCAL_TX_SIG_DOMAIN: &[u8] = b"novovm_local_tx_v1";

#[derive(Debug, Clone)]
struct TxGenArgs {
    out: PathBuf,
    txs: u32,
    accounts: u32,
    start_account: u64,
    start_key: u64,
    start_value: u64,
    fee_base: u64,
}

fn usage() -> &'static str {
    "usage: novovm-txgen --out <path> [--txs <n>] [--accounts <n>] [--start-account <u64>] [--start-key <u64>] [--start-value <u64>] [--fee-base <u64>]"
}

fn parse_u32(flag: &str, v: &str) -> Result<u32> {
    v.parse::<u32>()
        .with_context(|| format!("invalid value for {flag}: {v}"))
}

fn parse_u64(flag: &str, v: &str) -> Result<u64> {
    v.parse::<u64>()
        .with_context(|| format!("invalid value for {flag}: {v}"))
}

fn parse_args() -> Result<TxGenArgs> {
    let mut out: Option<PathBuf> = None;
    let mut txs: u32 = 10000;
    let mut accounts: u32 = 1024;
    let mut start_account: u64 = 1000;
    let mut start_key: u64 = 42;
    let mut start_value: u64 = 7;
    let mut fee_base: u64 = 1;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => {
                let v = it.next().context("--out requires a value")?;
                out = Some(PathBuf::from(v));
            }
            "--txs" => {
                let v = it.next().context("--txs requires a value")?;
                txs = parse_u32("--txs", &v)?;
            }
            "--accounts" => {
                let v = it.next().context("--accounts requires a value")?;
                accounts = parse_u32("--accounts", &v)?;
            }
            "--start-account" => {
                let v = it.next().context("--start-account requires a value")?;
                start_account = parse_u64("--start-account", &v)?;
            }
            "--start-key" => {
                let v = it.next().context("--start-key requires a value")?;
                start_key = parse_u64("--start-key", &v)?;
            }
            "--start-value" => {
                let v = it.next().context("--start-value requires a value")?;
                start_value = parse_u64("--start-value", &v)?;
            }
            "--fee-base" => {
                let v = it.next().context("--fee-base requires a value")?;
                fee_base = parse_u64("--fee-base", &v)?;
            }
            "--help" | "-h" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            _ => bail!("unknown argument: {arg}\n{}", usage()),
        }
    }

    let out = out.context(format!("missing --out\n{}", usage()))?;
    if txs == 0 {
        bail!("--txs must be > 0");
    }
    if accounts == 0 {
        bail!("--accounts must be > 0");
    }
    Ok(TxGenArgs {
        out,
        txs,
        accounts,
        start_account,
        start_key,
        start_value,
        fee_base,
    })
}

fn compute_signature(account: u64, key: u64, value: u64, nonce: u64, fee: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(LOCAL_TX_SIG_DOMAIN);
    hasher.update(account.to_le_bytes());
    hasher.update(key.to_le_bytes());
    hasher.update(value.to_le_bytes());
    hasher.update(nonce.to_le_bytes());
    hasher.update(fee.to_le_bytes());
    hasher.finalize().into()
}

fn build_tx(args: &TxGenArgs, i: u32) -> LocalTxWireV1 {
    let account_idx = i % args.accounts;
    let account = args.start_account + account_idx as u64;
    let nonce = (i / args.accounts) as u64;
    let fee = args.fee_base + (i % 5) as u64;
    let key = args.start_key + i as u64;
    let value = args.start_value + i as u64;
    let signature = compute_signature(account, key, value, nonce, fee);
    LocalTxWireV1 {
        account,
        key,
        value,
        nonce,
        fee,
        signature,
    }
}

fn main() -> Result<()> {
    let args = parse_args()?;
    if let Some(parent) = args.out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create output dir {}", parent.display()))?;
        }
    }

    let mut out = Vec::with_capacity(args.txs as usize * 77);
    for i in 0..args.txs {
        let tx = build_tx(&args, i);
        out.extend_from_slice(&encode_local_tx_wire_v1(&tx));
    }
    fs::write(&args.out, &out)
        .with_context(|| format!("failed to write tx wire file {}", args.out.display()))?;

    println!(
        "txgen_out: codec={} out={} txs={} accounts={} bytes={}",
        LOCAL_TX_WIRE_V1_CODEC,
        args.out.display(),
        args.txs,
        args.accounts,
        out.len()
    );
    Ok(())
}
