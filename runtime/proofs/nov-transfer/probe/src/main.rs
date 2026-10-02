//! Explicit, isolated worker diagnostic. This is not a node, activation policy,
//! or production throughput benchmark. Never run synchronous proving in poll.

use anyhow::{bail, ensure, Context, Result};
use novovm_aoem::{ReceiptBackendUnavailable, ReceiptLimits, ReceiptSession};
use novovm_host::proof::{JOURNAL_BYTES, MAX_INPUT_BYTES};
use novovm_transfer_methods::{NOVOVM_TRANSFER_GUEST_ELF, NOVOVM_TRANSFER_GUEST_ID};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;

mod auth_conformance;
mod fixture;

fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    ensure!(
        file.metadata()?.len() <= maximum as u64,
        "file exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= maximum, "file grew beyond bound");
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn journal(path: &Path) -> Result<Vec<u8>> {
    let bytes = read_bounded(path, JOURNAL_BYTES)?;
    ensure!(
        bytes.len() == JOURNAL_BYTES && bytes.starts_with(b"NVEXEC01"),
        "expected journal format mismatch"
    );
    Ok(bytes)
}

fn require_image() -> Result<()> {
    ensure!(
        !NOVOVM_TRANSFER_GUEST_ELF.is_empty() && NOVOVM_TRANSFER_GUEST_ID != [0; 8],
        "guest not built; RISC0_SKIP_BUILD cannot authorize proving/verification"
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let command = args.first().and_then(|s| s.to_str()).unwrap_or("");
    match (command, args.len()) {
        ("fixture", 3) => fixture::create(Path::new(&args[1]), Path::new(&args[2])),
        ("auth-conformance", 3) => {
            auth_conformance::run(Path::new(&args[1]), Path::new(&args[2]))
        }
        ("auth-conformance-verify", 4) => auth_conformance::verify(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
        ),
        ("image", 1) => {
            require_image()?;
            println!("image={:?} elf_bytes={} elf_sha256={}", NOVOVM_TRANSFER_GUEST_ID,
                NOVOVM_TRANSFER_GUEST_ELF.len(), hash(NOVOVM_TRANSFER_GUEST_ELF));
            Ok(())
        }
        ("prove", 5) => {
            require_image()?;
            ensure!(!Path::new(&args[4]).exists(), "receipt output already exists");
            let input = read_bounded(Path::new(&args[2]), MAX_INPUT_BYTES)?;
            let expected = journal(Path::new(&args[3]))?;
            let mut framed = Vec::with_capacity(input.len() + 4);
            framed.extend_from_slice(&u32::try_from(input.len())?.to_le_bytes());
            framed.extend_from_slice(&input);
            let mut session = ReceiptSession::open(Path::new(&args[1]), ReceiptLimits::default())?;
            let started = Instant::now();
            let receipt = session.prove(NOVOVM_TRANSFER_GUEST_ELF, &framed, &NOVOVM_TRANSFER_GUEST_ID)?;
            let prove = started.elapsed();
            let started = Instant::now();
            session.verify(&receipt, &NOVOVM_TRANSFER_GUEST_ID, &expected)?;
            let verify = started.elapsed();
            write_new(Path::new(&args[4]), &receipt)?;
            println!("proved_and_verified=true prove_ms={} verify_ms={} receipt_bytes={} receipt_sha256={} input_sha256={} expected_journal_sha256={}",
                prove.as_millis(), verify.as_millis(), receipt.len(), hash(&receipt), hash(&input), hash(&expected));
            Ok(())
        }
        ("verify", 4) | ("verify-negatives", 4) => {
            require_image()?;
            let receipt = read_bounded(Path::new(&args[2]), ReceiptLimits::default().receipt_bytes)?;
            // This file is an operator-supplied trusted expectation exported
            // from the native candidate, NOT data taken from this receipt.
            let expected = journal(Path::new(&args[3]))?;
            let mut session = ReceiptSession::open(Path::new(&args[1]), ReceiptLimits::default())?;
            let started = Instant::now();
            session.verify(&receipt, &NOVOVM_TRANSFER_GUEST_ID, &expected)?;
            println!("independent_verify=true verify_ms={} receipt_sha256={} expected_journal_sha256={}",
                started.elapsed().as_millis(), hash(&receipt), hash(&expected));
            if command == "verify-negatives" {
                negative_checks(&mut session, &receipt, &expected)?;
            }
            Ok(())
        }
        _ => bail!("usage: fixture <trusted-compute-lib> <new-dir> | image | prove <trusted-proof-lib> <input> <trusted-journal> <new-receipt> | verify[-negatives] <trusted-proof-lib> <receipt> <trusted-journal> | auth-conformance <trusted-proof-lib> <new-dir> | auth-conformance-verify <trusted-proof-lib> <receipt> <trusted-journal>"),
    }
}

fn negative_checks(session: &mut ReceiptSession, receipt: &[u8], expected: &[u8]) -> Result<()> {
    let mut rejected = 0;
    // Domain, every root/commitment and both counters must be bound.
    for index in [0, 8, 40, 72, 104, 136, 144] {
        let mut wrong = expected.to_vec();
        wrong[index] ^= 1;
        require_rejection(session.verify(receipt, &NOVOVM_TRANSFER_GUEST_ID, &wrong))?;
        rejected += 1;
    }
    for wrong in [Vec::new(), [expected, &[0]].concat()] {
        require_rejection(session.verify(receipt, &NOVOVM_TRANSFER_GUEST_ID, &wrong))?;
        rejected += 1;
    }
    let mut image = NOVOVM_TRANSFER_GUEST_ID;
    image[0] ^= 1;
    require_rejection(session.verify(receipt, &image, expected))?;
    rejected += 1;
    let mut wrong = receipt.to_vec();
    let last = wrong.len().checked_sub(1).context("empty receipt")?;
    wrong[last] ^= 1;
    require_rejection(session.verify(&wrong, &NOVOVM_TRANSFER_GUEST_ID, expected))?;
    require_rejection(session.verify(&receipt[..last], &NOVOVM_TRANSFER_GUEST_ID, expected))?;
    rejected += 2;
    let mut old_envelope = receipt.to_vec();
    old_envelope[..8].copy_from_slice(b"AORCP001");
    require_rejection(session.verify(&old_envelope, &NOVOVM_TRANSFER_GUEST_ID, expected))?;
    require_rejection(session.verify(
        &[receipt, &[0]].concat(),
        &NOVOVM_TRANSFER_GUEST_ID,
        expected,
    ))?;
    rejected += 2;
    // Negative requests must not turn a failed/poisoned backend into false
    // evidence that later mutations were cryptographically rejected.
    session.verify(receipt, &NOVOVM_TRANSFER_GUEST_ID, expected)?;
    println!("negative_verifications_rejected={rejected}");
    Ok(())
}

fn require_rejection(result: Result<()>) -> Result<()> {
    let error = result.err().context("invalid proof/output was accepted")?;
    ensure!(
        error.downcast_ref::<ReceiptBackendUnavailable>().is_none(),
        "unavailable backend is NOT evidence of cryptographic rejection"
    );
    Ok(())
}
