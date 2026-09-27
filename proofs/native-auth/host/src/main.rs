//! Diagnostic only. Verifier pins its own compiled guest and fixed statement.
//! No complete transaction validity, state root, block seal or finality claim.
use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signer, SigningKey};
use novovm_adapter_api::{
    native_signing::{native_signer_address_v1, tx_signing_message_v1},
    TxIR, TxType,
};
use novovm_auth_methods::{NOVOVM_AUTH_GUEST_ELF, NOVOVM_AUTH_GUEST_ID};
use novovm_auth_statement::{check, AuthInput, AuthJournal};
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use std::{fs, io::Write, path::Path, process::Command};
fn fixture() -> AuthInput {
    let key = SigningKey::from_bytes(&[7; 32]);
    let mut tx = TxIR {
        hash: vec![0x41; 32],
        from: native_signer_address_v1(&key.verifying_key().to_bytes()).to_vec(),
        account_id: None,
        fee_owner_account_id: None,
        nonce_owner_account_id: None,
        to: Some(vec![3; 20]),
        value: 1,
        gas_limit: 21_000,
        gas_price: 1,
        nonce: 0,
        data: vec![],
        signature: vec![],
        chain_id: 1,
        tx_type: TxType::Transfer,
        execution_policy: Default::default(),
        evm_access_list: vec![],
        source_chain: None,
        target_chain: None,
    };
    tx.signature = key.verifying_key().to_bytes().to_vec();
    tx.signature
        .extend_from_slice(&key.sign(&tx_signing_message_v1(&tx)).to_bytes());
    AuthInput {
        tx,
        expected_chain: 1,
        expected_nonce: 0,
    }
}

fn child(mode: &str, lib: &str, proof: &str) -> Result<()> {
    let mut command = Command::new(std::env::current_exe()?);
    command.args([mode, lib, proof]);
    command.env_remove("RISC0_DEV_MODE");
    if !command.status()?.success() {
        bail!("{mode} failed");
    }
    Ok(())
}

// Frozen public statement reviewed from the fixture. Verification needs neither
// private signing key nor signature witness, and never runs fixture()/check().
fn expected_statement() -> AuthJournal {
    AuthJournal {
        domain: "novovm-signature-nonce-relation/v1".into(),
        message: [
            13, 136, 12, 148, 22, 21, 172, 177, 165, 189, 91, 85, 8, 240, 100, 23, 217, 40, 157,
            66, 81, 44, 241, 253, 76, 154, 29, 64, 131, 215, 81, 99,
        ],
        public_key: [
            234, 74, 108, 99, 226, 156, 82, 10, 190, 245, 80, 123, 19, 46, 197, 249, 149, 71, 118,
            174, 190, 190, 123, 146, 66, 30, 234, 105, 20, 70, 210, 44,
        ],
        nonce_identity: [
            115, 100, 11, 247, 216, 198, 138, 220, 65, 23, 63, 79, 103, 185, 146, 94, 209, 97, 99,
            246, 26, 30, 163, 144, 228, 84, 160, 6, 4, 111, 68, 7,
        ],
        chain: 1,
        nonce: 0,
        next_nonce: 1,
    }
}

#[test]
fn frozen_public_statement_matches_signed_fixture() {
    assert_eq!(check(&fixture()).unwrap(), expected_statement());
}
fn rejected(result: Result<()>, label: &str) -> Result<()> {
    if result.is_ok() {
        bail!("accepted {label}");
    }
    Ok(())
}
fn main() -> Result<()> {
    if NOVOVM_AUTH_GUEST_ELF.is_empty() || NOVOVM_AUTH_GUEST_ID == [0; 8] {
        bail!("guest not built: rebuild without RISC0_SKIP_BUILD");
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [mode, lib, output] = args.as_slice() else {
        bail!("usage: run|produce|verify <trusted-AOEM-library> <new-output-dir|receipt>");
    };
    if mode == "run" {
        fs::create_dir(output).context("output must be new; parent must exist")?;
        let proof = Path::new(output).join("receipt.bin");
        let proof = proof.to_str().context("non UTF-8 path")?;
        child("produce", lib, proof)?;
        child("verify", lib, proof)?;
        println!("PASS: real signature/nonce relation proof, independent verifier. Full NOV validity NOT PROVEN");
        return Ok(());
    }
    let host = AoemExecFacade::open(lib, AoemExecOpenOptions::default())?;
    let expected = postcard::to_allocvec(&expected_statement())?;
    match mode.as_str() {
        "produce" => {
            assert_eq!(
                check(&fixture()).map_err(anyhow::Error::msg)?,
                expected_statement()
            );
            // These are invalid witnesses, not just bad expected journals.
            for case in 0..3 {
                let mut bad = fixture();
                match case {
                    0 => bad.tx.signature[40] ^= 1,
                    1 => bad.expected_chain = 2,
                    _ => bad.expected_nonce = 1,
                }
                let bytes = postcard::to_allocvec(&bad)?;
                let input = [(bytes.len() as u32).to_le_bytes().as_slice(), &bytes].concat();
                rejected(
                    host.risc0_prove_v1(NOVOVM_AUTH_GUEST_ELF, &input, &NOVOVM_AUTH_GUEST_ID)
                        .map(|_| ()),
                    "invalid witness",
                )?;
            }
            let bytes = postcard::to_allocvec(&fixture())?;
            let input = [(bytes.len() as u32).to_le_bytes().as_slice(), &bytes].concat();
            let proof =
                host.risc0_prove_v1(NOVOVM_AUTH_GUEST_ELF, &input, &NOVOVM_AUTH_GUEST_ID)?;
            host.risc0_verify_v1(&proof, &NOVOVM_AUTH_GUEST_ID, &expected)?;
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(output)?;
            file.write_all(&proof)?;
            file.sync_all()?;
            println!(
                "producer PASS: {} bytes; image {:?}",
                proof.len(),
                NOVOVM_AUTH_GUEST_ID
            );
        }
        "verify" => {
            if fs::metadata(output)?.len() > 16 * 1024 * 1024 {
                bail!("receipt too large");
            }
            let proof = fs::read(output)?;
            // Expected statement and image come from this verifier, not the proof sender.
            host.risc0_verify_v1(&proof, &NOVOVM_AUTH_GUEST_ID, &expected)?;
            let mut wrong = expected.clone();
            *wrong.last_mut().context("empty journal")? ^= 1;
            rejected(
                host.risc0_verify_v1(&proof, &NOVOVM_AUTH_GUEST_ID, &wrong),
                "wrong statement",
            )?;
            rejected(
                host.risc0_verify_v1(&proof, &[0; 8], &expected),
                "wrong program",
            )?;
            let mut tampered = proof.clone();
            let n = tampered.len();
            tampered[n / 2] ^= 1;
            rejected(
                host.risc0_verify_v1(&tampered, &NOVOVM_AUTH_GUEST_ID, &expected),
                "tampered proof",
            )?;
            rejected(
                host.risc0_verify_v1(&proof[..proof.len() - 1], &NOVOVM_AUTH_GUEST_ID, &expected),
                "truncated proof",
            )?;
            println!("independent verifier PASS (no witness/ELF supplied by producer)");
        }
        _ => bail!("unknown mode"),
    }
    Ok(())
}
