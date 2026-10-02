//! Real guest/native strict-authentication comparison. Fixed keys below are
//! public test fixtures, never production credentials. No execution witness or
//! balance failure can substitute for the signature checks in this diagnostic.

use anyhow::{ensure, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use novovm_aoem::{ReceiptLimits, ReceiptSession};
use novovm_host::ingress::{
    authentication::authenticate_transfer_v3,
    wire::{decode_transfer_v3, encode_transfer_v3, signing_message, FeePolicy, TransferV3},
};
use novovm_transfer_methods::{
    NOVOVM_AUTH_CONFORMANCE_GUEST_ELF, NOVOVM_AUTH_CONFORMANCE_GUEST_ID,
};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Instant};

const CHAIN_ID: u64 = 71;
const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_CASES: usize = 32;
const MAX_RAW_BYTES: usize = 4096;
const INPUT_MAGIC: &[u8; 8] = b"NVAUTHI1";
const JOURNAL_MAGIC: &[u8; 8] = b"NVAUTHJ1";
const CASE_COUNT: usize = 16;
const EXPECTED_RESULTS: [u8; CASE_COUNT] = [1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const JOURNAL_BYTES: usize = 42 + CASE_COUNT;
const SCALAR_ORDER: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
];

#[derive(Clone, Copy)]
enum Expected {
    Accept,
    RejectSignature,
    RejectPublicKey,
}

impl Expected {
    fn success(self) -> bool {
        matches!(self, Self::Accept)
    }
}

struct Case {
    name: &'static str,
    transaction: TransferV3,
    expected: Expected,
}

fn account(public_key: &[u8; 32], width: usize) -> Vec<u8> {
    match width {
        20 => Sha256::digest(public_key)[12..32].to_vec(),
        32 => public_key.to_vec(),
        _ => unreachable!("fixed fixture width"),
    }
}

fn signed(seed: u8, width: usize, nonce: u64, amount: u128) -> Result<TransferV3> {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let recipient = SigningKey::from_bytes(&[91; 32]).verifying_key();
    let mut transaction = TransferV3 {
        chain_id: CHAIN_ID,
        from: account(key.verifying_key().as_bytes(), width),
        to: account(recipient.as_bytes(), width),
        asset: "NOV".into(),
        amount,
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 300,
            slippage_bps: 25,
        },
        signature: Vec::new(),
    };
    let signature = key.sign(&signing_message(&transaction)?);
    transaction
        .signature
        .extend_from_slice(key.verifying_key().as_bytes());
    transaction
        .signature
        .extend_from_slice(&signature.to_bytes());
    Ok(transaction)
}

fn cases() -> Result<Vec<Case>> {
    let mut cases = Vec::with_capacity(CASE_COUNT);
    for (name, seed, width, nonce, amount) in [
        ("valid-20-key11-message0", 11, 20, 0, 19),
        ("valid-32-key11-message1", 11, 32, 1, 20),
        ("valid-20-key12-message2", 12, 20, 2, 21),
        ("valid-32-key12-message3", 12, 32, 3, 22),
        ("valid-20-key13-message4", 13, 20, 4, 23),
        ("valid-32-key13-message5", 13, 32, 5, 24),
    ] {
        cases.push(Case {
            name,
            transaction: signed(seed, width, nonce, amount)?,
            expected: Expected::Accept,
        });
    }
    let original = signed(17, 32, 6, 25)?;
    let mut bad_signature = original.clone();
    bad_signature.signature[64] ^= 1;
    cases.push(Case {
        name: "modified-signature-scalar",
        transaction: bad_signature,
        expected: Expected::RejectSignature,
    });
    let mut bad_message = original.clone();
    bad_message.amount += 1;
    cases.push(Case {
        name: "modified-signed-amount",
        transaction: bad_message,
        expected: Expected::RejectSignature,
    });
    let mut scalar_order = original.clone();
    scalar_order.signature[64..96].copy_from_slice(&SCALAR_ORDER);
    cases.push(Case {
        name: "noncanonical-s-equals-l",
        transaction: scalar_order,
        expected: Expected::RejectSignature,
    });
    let mut scalar_plus_order = original.clone();
    let mut carry = 0_u16;
    for (byte, order) in scalar_plus_order.signature[64..96]
        .iter_mut()
        .zip(SCALAR_ORDER)
    {
        let sum = u16::from(*byte) + u16::from(order) + carry;
        *byte = sum as u8;
        carry = sum >> 8;
    }
    ensure!(carry == 0, "fixed S+L fixture overflowed");
    cases.push(Case {
        name: "noncanonical-valid-s-plus-l",
        transaction: scalar_plus_order,
        expected: Expected::RejectSignature,
    });

    let mut identity = [0; 32];
    identity[0] = 1;
    let mut weak_r = original.clone();
    weak_r.signature[32..64].copy_from_slice(&identity);
    cases.push(Case {
        name: "small-order-identity-r",
        transaction: weak_r,
        expected: Expected::RejectSignature,
    });
    // y=p+1 is a noncanonical encoding of the identity, not an arbitrary
    // corrupted transaction carrier. Strict verification must reject it.
    let mut noncanonical_r = [0xff; 32];
    noncanonical_r[0] = 0xee;
    noncanonical_r[31] = 0x7f;
    let mut alternate_r = original.clone();
    alternate_r.signature[32..64].copy_from_slice(&noncanonical_r);
    cases.push(Case {
        name: "noncanonical-r-y-p-plus-one",
        transaction: alternate_r,
        expected: Expected::RejectSignature,
    });

    // Deterministically select the first invalid fixed repeated-byte point.
    // Native decoding must actually reject it; no guessed compressed encoding
    // is silently accepted as an alleged negative test.
    let invalid_point = (0..=u8::MAX)
        .map(|byte| [byte; 32])
        .find(|bytes| VerifyingKey::from_bytes(bytes).is_err())
        .context("no invalid fixed compressed point found")?;
    let mut invalid_r = original.clone();
    invalid_r.signature[32..64].copy_from_slice(&invalid_point);
    cases.push(Case {
        name: "nondecompressible-r",
        transaction: invalid_r,
        expected: Expected::RejectSignature,
    });
    let mut invalid_a = original;
    invalid_a.from = account(&invalid_point, 32);
    invalid_a.signature[..32].copy_from_slice(&invalid_point);
    cases.push(Case {
        name: "nondecompressible-a",
        transaction: invalid_a,
        expected: Expected::RejectPublicKey,
    });

    let weak_key = VerifyingKey::from_bytes(&identity)?;
    ensure!(weak_key.is_weak(), "identity fixture is not a weak key");
    let mut identity_signature = [0; 64];
    identity_signature[..32].copy_from_slice(&identity);
    for (name, width) in [
        ("identity-a-basepoint-r-s1-20", 20),
        ("identity-a-r-s0-32", 32),
    ] {
        let mut signature_bytes = identity_signature;
        if width == 20 {
            // A=identity, R=B, S=1 satisfies the ordinary equation for every
            // message. R is not small-order: this isolates the weak-A gate.
            signature_bytes[..32].fill(0x66);
            signature_bytes[0] = 0x58;
            signature_bytes[32] = 1;
        }
        let signature = Signature::from_bytes(&signature_bytes);
        let mut transaction = signed(18, width, 7, 26)?;
        transaction.from = account(&identity, width);
        transaction.signature[..32].copy_from_slice(&identity);
        transaction.signature[32..].copy_from_slice(&signature_bytes);
        let message = signing_message(&transaction)?;
        weak_key
            .verify(&message, &signature)
            .context("ordinary verification must accept the weak-key counterexample")?;
        ensure!(
            weak_key.verify_strict(&message, &signature).is_err(),
            "native strict verification accepted the weak-key counterexample"
        );
        cases.push(Case {
            name,
            transaction,
            expected: Expected::RejectSignature,
        });
    }
    ensure!(cases.len() == CASE_COUNT, "case catalog/count diverged");
    Ok(cases)
}

fn fixture() -> Result<(Vec<u8>, Vec<u8>)> {
    let cases = cases()?;
    ensure!(
        (1..=MAX_CASES).contains(&cases.len()),
        "case limit exceeded"
    );
    let mut input = INPUT_MAGIC.to_vec();
    input.extend_from_slice(&CHAIN_ID.to_le_bytes());
    input.extend_from_slice(&u16::try_from(cases.len())?.to_le_bytes());
    let mut outcomes = Vec::with_capacity(cases.len());
    for case in cases {
        let raw = encode_transfer_v3(&case.transaction)?;
        ensure!(
            !raw.is_empty() && raw.len() <= MAX_RAW_BYTES,
            "raw fixture exceeds bound"
        );
        let decoded = decode_transfer_v3(&raw, MAX_RAW_BYTES)?;
        ensure!(
            decoded == case.transaction,
            "fixture canonical roundtrip changed"
        );
        let key: [u8; 32] = decoded.signature[..32].try_into()?;
        ensure!(
            decoded.chain_id == CHAIN_ID
                && decoded.nonce != u64::MAX
                && matches!(decoded.to.len(), 20 | 32)
                && decoded.from == account(&key, decoded.from.len()),
            "fixture fails a non-signature prerequisite: {}",
            case.name
        );
        let result = authenticate_transfer_v3(&raw, CHAIN_ID, MAX_RAW_BYTES);
        match case.expected {
            Expected::Accept => {
                result.with_context(|| format!("native positive failed: {}", case.name))?;
            }
            Expected::RejectSignature | Expected::RejectPublicKey => {
                let error = result.err().with_context(|| {
                    format!("native negative unexpectedly accepted: {}", case.name)
                })?;
                let expected_reason = match case.expected {
                    Expected::RejectSignature => "invalid native V3 signature",
                    Expected::RejectPublicKey => "invalid Ed25519 public key",
                    Expected::Accept => unreachable!(),
                };
                ensure!(
                    error.to_string() == expected_reason,
                    "negative failed outside its intended cryptographic gate: {}: {error:#}",
                    case.name
                );
            }
        }
        input.extend_from_slice(&u32::try_from(raw.len())?.to_le_bytes());
        input.extend_from_slice(&raw);
        outcomes.push(u8::from(case.expected.success()));
        println!(
            "native_auth_case={} expected_success={}",
            case.name,
            case.expected.success()
        );
    }
    ensure!(
        input.len() <= MAX_INPUT_BYTES - 4,
        "input exceeds total bound"
    );
    ensure!(
        outcomes == EXPECTED_RESULTS,
        "explicit result catalog diverged"
    );
    let mut journal = JOURNAL_MAGIC.to_vec();
    journal.extend_from_slice(&Sha256::digest(&input));
    journal.extend_from_slice(&(CASE_COUNT as u16).to_le_bytes());
    journal.extend_from_slice(&outcomes);
    Ok((input, journal))
}

fn require_image() -> Result<()> {
    ensure!(
        !NOVOVM_AUTH_CONFORMANCE_GUEST_ELF.is_empty() && NOVOVM_AUTH_CONFORMANCE_GUEST_ID != [0; 8],
        "auth conformance guest not built; skipped builds cannot authorize verification"
    );
    Ok(())
}

fn limits() -> ReceiptLimits {
    ReceiptLimits {
        input_bytes: MAX_INPUT_BYTES,
        journal_bytes: JOURNAL_BYTES,
        ..ReceiptLimits::default()
    }
}

fn validate_journal(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() == JOURNAL_BYTES
            && bytes.starts_with(JOURNAL_MAGIC)
            && bytes[40..42] == (CASE_COUNT as u16).to_le_bytes()
            && bytes[42..] == EXPECTED_RESULTS,
        "expected auth journal format or explicit result catalog mismatch"
    );
    Ok(())
}

/// A standalone producer. The files are public diagnostic fixtures, not wallet
/// keys. A failure may leave inputs, but no receipt is published before verify.
pub(super) fn run(library: &Path, directory: &Path) -> Result<()> {
    require_image()?;
    let (input, expected) = fixture()?;
    validate_journal(&expected)?;
    std::fs::create_dir(directory).context("create new conformance output directory")?;
    super::write_new(&directory.join("input.bin"), &input)?;
    super::write_new(&directory.join("expected-journal.bin"), &expected)?;
    let mut framed = Vec::with_capacity(input.len() + 4);
    framed.extend_from_slice(&u32::try_from(input.len())?.to_le_bytes());
    framed.extend_from_slice(&input);
    let mut session = ReceiptSession::open(library, limits())?;
    let started = Instant::now();
    let receipt = session.prove(
        NOVOVM_AUTH_CONFORMANCE_GUEST_ELF,
        &framed,
        &NOVOVM_AUTH_CONFORMANCE_GUEST_ID,
    )?;
    let prove_ms = started.elapsed().as_millis();
    session.verify(&receipt, &NOVOVM_AUTH_CONFORMANCE_GUEST_ID, &expected)?;
    super::write_new(&directory.join("receipt.bin"), &receipt)?;
    println!(
        "auth_conformance_proved_and_verified=true cases={CASE_COUNT} accepted=6 rejected=10 prove_ms={prove_ms} receipt_bytes={} receipt_sha256={} input_sha256={} expected_journal_sha256={}",
        receipt.len(), super::hash(&receipt), super::hash(&input), super::hash(&expected)
    );
    Ok(())
}

/// Independent consumer: no fixture construction, signing or proving. The
/// operator supplies the expected journal saved by the native producer.
pub(super) fn verify(library: &Path, receipt: &Path, expected: &Path) -> Result<()> {
    require_image()?;
    let receipt = super::read_bounded(receipt, limits().receipt_bytes)?;
    let expected = super::read_bounded(expected, JOURNAL_BYTES)?;
    validate_journal(&expected)?;
    let mut session = ReceiptSession::open(library, limits())?;
    let started = Instant::now();
    session.verify(&receipt, &NOVOVM_AUTH_CONFORMANCE_GUEST_ID, &expected)?;
    let verify_ms = started.elapsed().as_millis();
    let mut wrong_output = expected.clone();
    wrong_output[42] ^= 1;
    super::require_rejection(session.verify(
        &receipt,
        &NOVOVM_AUTH_CONFORMANCE_GUEST_ID,
        &wrong_output,
    ))?;
    let mut wrong_image = NOVOVM_AUTH_CONFORMANCE_GUEST_ID;
    wrong_image[0] ^= 1;
    super::require_rejection(session.verify(&receipt, &wrong_image, &expected))?;
    // Rejection cannot be attributed to an unavailable/poisoned backend: the
    // exact original request must still verify after both negative checks.
    session.verify(&receipt, &NOVOVM_AUTH_CONFORMANCE_GUEST_ID, &expected)?;
    println!(
        "independent_auth_conformance_verify=true cases={CASE_COUNT} wrong_output_rejected=true wrong_image_rejected=true verify_ms={verify_ms} receipt_sha256={} expected_journal_sha256={}",
        super::hash(&receipt), super::hash(&expected)
    );
    Ok(())
}
