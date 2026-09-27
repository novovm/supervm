//! Opt-in V2 proof probe. Trusted expectations are prepared by the controller,
//! not exported by the prover. This fixture is NOT a finalized/live parent.
use anyhow::{bail, Context, Result};
use novovm_auth_methods::{
    NOVOVM_PARENT_NONCE_GUEST_ELF as ELF, NOVOVM_PARENT_NONCE_GUEST_ID as IMAGE,
};
use novovm_auth_statement::parent::{
    check_parent, ParentAuthInput, ParentAuthJournal, MAX_PARENT_INPUT_BYTES,
};
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    process::Command,
};

fn read(path: impl AsRef<Path>, max: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max {
        bail!("input exceeds bound");
    }
    Ok(bytes)
}
fn write_new(path: impl AsRef<Path>, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn input(bytes: &[u8]) -> Result<ParentAuthInput> {
    let (value, rest) = postcard::take_from_bytes(bytes)?;
    if !rest.is_empty() {
        bail!("trailing witness");
    }
    Ok(value)
}
fn stdin(bytes: &[u8]) -> Vec<u8> {
    [(bytes.len() as u32).to_le_bytes().as_slice(), bytes].concat()
}
fn reject(result: Result<()>, label: &str) -> Result<()> {
    if result.is_ok() {
        bail!("accepted {label}");
    }
    println!("rejected: {label}");
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode, directory, chain] if mode == "prepare" => {
            let directory = Path::new(directory);
            let parent_root = read(directory.join("parent.root"), 32)?.try_into()
                .map_err(|_| anyhow::anyhow!("root must have 32 bytes"))?;
            let tx_bytes = read(directory.join("tx.postcard"), 64*1024)?;
            let (tx, rest) = postcard::take_from_bytes(&tx_bytes)?;
            if !rest.is_empty() { bail!("trailing transaction"); }
            let witness = ParentAuthInput {
                tx, expected_chain: chain.parse()?, parent_root,
                parent_state_wire: read(directory.join("parent.wire"), 1024*1024)?,
            };
            let journal = check_parent(&witness).map_err(anyhow::Error::msg)?;
            let bytes = postcard::to_allocvec(&witness)?;
            if bytes.len() > MAX_PARENT_INPUT_BYTES { bail!("witness too large"); }
            // Must be done by the trusted controller before untrusted proving.
            write_new(directory.join("expected.journal"), &postcard::to_allocvec(&journal)?)?;
            write_new(directory.join("witness.postcard"), &bytes)?;
            println!("prepared trusted fixture expectations; live_parent=false");
        }
        [mode, lib, directory] if mode == "run" => {
            let executable = std::env::current_exe()?;
            let directory = Path::new(directory);
            let witness = directory.join("witness.postcard");
            let expected = directory.join("expected.journal");
            let receipt = directory.join("receipt.bin");
            for (mode, input) in [("produce", witness), ("verify", expected)] {
                let mut child = Command::new(&executable);
                child.arg(mode).arg(lib).arg(input).arg(&receipt);
                if mode == "produce" { child.env_remove("RISC0_DEV_MODE"); }
                else { child.env("RISC0_DEV_MODE", "1"); }
                if !child.status()?.success() { bail!("{mode} child failed"); }
            }
            println!("PASS: V2 node-encoded fixture cross-process proof; live parent/finality NOT PROVEN");
        }
        [mode, lib, source, receipt] if mode == "produce" || mode == "verify" => {
            if ELF.is_empty() || IMAGE == [0;8] { bail!("guest not built"); }
            let host = AoemExecFacade::open(lib, AoemExecOpenOptions::default())?;
            if mode == "produce" {
                if Path::new(receipt).exists() {
                    bail!("receipt output must be new");
                }
                let bytes = read(source, MAX_PARENT_INPUT_BYTES)?;
                let witness = input(&bytes)?;
                let expected = postcard::to_allocvec(&check_parent(&witness).map_err(anyhow::Error::msg)?)?;
                for case in 0..3 {
                    let mut bad = witness.clone();
                    match case {
                        0 => bad.parent_root[0] ^= 1,
                        1 => { *bad.parent_state_wire.last_mut().context("empty state")? ^= 1; }
                        _ => bad.tx.signature[40] ^= 1,
                    }
                    reject(host.risc0_prove_v1(ELF, &stdin(&postcard::to_allocvec(&bad)?), &IMAGE).map(|_| ()), "invalid parent/signature witness")?;
                }
                let proof = host.risc0_prove_v1(ELF, &stdin(&bytes), &IMAGE)?;
                host.risc0_verify_v1(&proof, &IMAGE, &expected)?;
                write_new(receipt, &proof)?;
                println!("producer PASS bytes={} image={IMAGE:?}", proof.len());
            } else {
                // Only local trusted expectation + receipt: no snapshot, TxIR or
                // signature witness is read by the verifier.
                let expected = read(source, 4096)?;
                let proof = read(receipt, 16*1024*1024)?;
                host.risc0_verify_v1(&proof, &IMAGE, &expected)?;
                for case in 0..5 {
                    let (mut bad, rest): (ParentAuthJournal, _) = postcard::take_from_bytes(&expected)?;
                    if !rest.is_empty() { bail!("trailing expected journal"); }
                    match case {
                        0 => bad.parent_root[0] ^= 1,
                        1 => bad.auth.nonce ^= 1,
                        2 => bad.auth.chain ^= 1,
                        3 => bad.auth.message[0] ^= 1,
                        _ => bad.auth.public_key[0] ^= 1,
                    }
                    reject(host.risc0_verify_v1(&proof, &IMAGE, &postcard::to_allocvec(&bad)?), "wrong public statement")?;
                }
                reject(host.risc0_verify_v1(&proof, &[0;8], &expected), "wrong image")?;
                reject(host.risc0_verify_v1(&proof, &IMAGE, &[]), "empty statement")?;
                let mut bad = proof.clone();
                let n = bad.len();
                bad[n/2] ^= 1;
                reject(host.risc0_verify_v1(&bad, &IMAGE, &expected), "tampered receipt")?;
                reject(host.risc0_verify_v1(&proof[..proof.len()-1], &IMAGE, &expected), "truncated receipt")?;
                println!("independent V2 verifier PASS");
            }
        }
        _ => bail!("usage: prepare <trusted-node-export-dir> <chain> | run <library> <prepared-dir> | produce <library> <witness> <new-receipt> | verify <library> <trusted-expected-journal> <receipt>"),
    }
    Ok(())
}
