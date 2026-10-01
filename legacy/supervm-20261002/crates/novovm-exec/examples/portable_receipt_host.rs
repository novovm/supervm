//! Opt-in real AOEM portability test through the production host facade.
//! Uses the existing Fibonacci fixture, NOT a NOV transaction guest.
//! run LIB ELF IMAGE_WORDS_CSV NEW_OUTPUT_DIR
//! unavailable LIB (old library or backend disabled)
use anyhow::{bail, Context, Result};
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use std::{fs, io::Write, path::Path, process::Command};

fn image(raw: &str) -> Result<[u32; 8]> {
    raw.split(',')
        .map(str::parse)
        .collect::<std::result::Result<Vec<u32>, _>>()?
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected eight image-ID words"))
}

fn open(path: &str) -> Result<AoemExecFacade> {
    AoemExecFacade::open(path, AoemExecOpenOptions::default())
}

fn require_rejected(result: Result<()>, label: &str) -> Result<()> {
    if result.is_ok() {
        bail!("unexpectedly accepted {label}");
    }
    Ok(())
}

fn child(args: &[&str], producer: bool) -> Result<()> {
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.args(args);
    if producer {
        cmd.env_remove("RISC0_DEV_MODE");
    } else {
        cmd.env("RISC0_DEV_MODE", "1");
    }
    if !cmd.status()?.success() {
        bail!("child {} failed", args[0]);
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode, library, elf, pin, output] if mode == "run" => {
            let _ = image(pin)?;
            fs::create_dir(output)
                .context("output directory must be new, with an existing parent")?;
            let proof = Path::new(output).join("receipt.bin");
            let proof = proof.to_str().context("non UTF-8 proof path")?;
            child(&["produce", library, elf, pin, proof], true)?;
            // Producer has exited. No ELF or witness passed to the verifier.
            child(&["verify", library, pin, proof], false)?;
            println!("PASS: host facade cross-process portable receipt; NOV transaction proof NOT IMPLEMENTED");
        }
        [mode, library, elf, pin, output] if mode == "produce" => {
            let host = open(library)?;
            if !host.has_risc0_portable_exports_v1() {
                bail!("portable receipt exports missing");
            }
            let elf = fs::read(elf)?;
            let stdin = [
                0u64.to_le_bytes().as_slice(),
                1u64.to_le_bytes().as_slice(),
                10u32.to_le_bytes().as_slice(),
            ]
            .concat();
            require_rejected(
                host.risc0_prove_v1(&elf, &stdin, &[0; 8]).map(|_| ()),
                "wrong producer image",
            )?;
            let receipt = host.risc0_prove_v1(&elf, &stdin, &image(pin)?)?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            file.write_all(&receipt)?;
            file.sync_all()?;
            println!("producer: PASS ({} proof bytes)", receipt.len());
        }
        [mode, library, pin, input] if mode == "verify" => {
            let host = open(library)?;
            let proof = fs::read(input)?;
            let image = image(pin)?;
            let journal = 89u64.to_le_bytes();
            host.risc0_verify_v1(&proof, &image, &journal)?;
            require_rejected(
                host.risc0_verify_v1(&proof, &[0; 8], &journal),
                "wrong image",
            )?;
            require_rejected(
                host.risc0_verify_v1(&proof, &image, &90u64.to_le_bytes()),
                "wrong output",
            )?;
            require_rejected(host.risc0_verify_v1(&proof, &image, &[]), "omitted output")?;
            require_rejected(
                host.risc0_verify_v1(&proof[..proof.len() - 1], &image, &journal),
                "truncation",
            )?;
            let mut appended = proof.clone();
            appended.push(0);
            require_rejected(
                host.risc0_verify_v1(&appended, &image, &journal),
                "trailing data",
            )?;
            let mut tampered = proof;
            let middle = tampered.len() / 2;
            tampered[middle] ^= 1;
            require_rejected(
                host.risc0_verify_v1(&tampered, &image, &journal),
                "tampering",
            )?;
            println!("verifier: PASS (valid receipt accepted; six negative cases rejected)");
        }
        [mode, library] if mode == "unavailable" => {
            let host = open(library)?;
            let prove = host
                .risc0_prove_v1(b"x", &[], &[0; 8])
                .unwrap_err()
                .to_string();
            let verify = host
                .risc0_verify_v1(b"AORCP001", &[0; 8], &[])
                .unwrap_err()
                .to_string();
            for error in [prove, verify] {
                if !error.contains("not found") && !error.contains("rc=-5") {
                    bail!("expected unavailable, got {error}");
                }
            }
            println!(
                "unavailable: PASS; exports_present={} (not a readiness claim)",
                host.has_risc0_portable_exports_v1()
            );
        }
        _ => bail!("usage: run LIB ELF IMAGE_WORDS_CSV NEW_OUTPUT_DIR | unavailable LIB"),
    }
    Ok(())
}
