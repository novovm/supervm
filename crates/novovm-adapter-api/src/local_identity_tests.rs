use super::*;

const CREATED: u64 = 1_000_000;
const ISSUED: u64 = CREATED + 10;
const EPOCH: u64 = 7;

fn signer(seed: u8) -> LocalIdentitySignerV1 {
    LocalIdentitySignerV1::from_seed(Zeroizing::new([seed; 32])).unwrap()
}

fn request() -> LocalKeyAuthorizationRequestV1 {
    LocalKeyAuthorizationRequestV1 {
        purpose: [3; 32],
        device_binding: [4; 32],
        scope: [5; 32],
        subject_public_key: signer(2).public_key(),
        challenge: [6; 32],
        created_at_ms: CREATED,
    }
}

fn proof() -> (
    LocalIdentitySignerV1,
    LocalKeyAuthorizationRequestV1,
    LocalKeyAuthorizationV1,
) {
    let root = signer(1);
    let request = request();
    let proof = root
        .authorize(
            &request,
            EPOCH,
            ISSUED,
            ISSUED + MAX_LOCAL_AUTHORIZATION_LIFETIME_MS_V1,
        )
        .unwrap();
    (root, request, proof)
}

#[test]
fn signature_uses_existing_root_reference_and_preserves_all_bindings() {
    let (root, request, proof) = proof();
    let expected_ref =
        derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Ed25519, &root.public_key());
    assert_eq!(root.primary_key_ref().as_slice(), expected_ref);
    assert!(proof.request() == &request);
    assert_eq!(proof.epoch(), EPOCH);
    assert_eq!(proof.issued_at_ms(), ISSUED);
    let verified = proof
        .verify(root.public_key(), &request, EPOCH, ISSUED)
        .unwrap();
    assert!(verified.request() == &request);
    assert_eq!(verified.subject_public_key(), request.subject_public_key);
    assert_eq!(verified.primary_key_ref(), root.primary_key_ref());
    assert_eq!(verified.epoch(), EPOCH);
    assert_eq!(verified.issued_at_ms(), ISSUED);
    assert_eq!(verified.expires_at_ms(), proof.expires_at_ms());
    assert_eq!(proof.signing_bytes().len(), SIGNING_BYTES);
    assert!(proof.signing_bytes().starts_with(DOMAIN));
}

#[test]
fn request_freshness_is_at_issuance_not_a_five_minute_grant_lifetime() {
    let (root, request, proof) = proof();
    assert!(proof
        .verify(
            root.public_key(),
            &request,
            EPOCH,
            ISSUED + MAX_LOCAL_AUTHORIZATION_REQUEST_AGE_MS_V1 + 1
        )
        .is_ok());
    assert!(proof
        .verify(
            root.public_key(),
            &request,
            EPOCH,
            proof.expires_at_ms() - 1
        )
        .is_ok());
    assert!(proof
        .verify(root.public_key(), &request, EPOCH, proof.expires_at_ms())
        .is_err());
}

#[test]
fn every_expected_binding_and_trusted_root_epoch_are_required() {
    let (root, request, proof) = proof();
    let changes: &[fn(&mut LocalKeyAuthorizationRequestV1)] = &[
        |r| r.purpose[0] ^= 1,
        |r| r.device_binding[0] ^= 1,
        |r| r.scope[0] ^= 1,
        |r| r.subject_public_key = signer(8).public_key(),
        |r| r.challenge[0] ^= 1,
        |r| r.created_at_ms += 1,
    ];
    for change in changes {
        let mut different = request.clone();
        change(&mut different);
        assert!(proof
            .verify(root.public_key(), &different, EPOCH, ISSUED)
            .is_err());
    }
    assert!(proof
        .verify(signer(9).public_key(), &request, EPOCH, ISSUED)
        .is_err());
    for epoch in [0, EPOCH - 1, EPOCH + 1] {
        assert!(proof
            .verify(root.public_key(), &request, epoch, ISSUED)
            .is_err());
    }
}

#[test]
fn all_transcript_fields_are_cryptographically_bound_even_with_matching_expectations() {
    let (root, _, proof) = proof();
    let changes: &[fn(&mut LocalKeyAuthorizationV1)] = &[
        |p| p.request.purpose[0] ^= 1,
        |p| p.request.device_binding[0] ^= 1,
        |p| p.request.scope[0] ^= 1,
        |p| p.request.subject_public_key = signer(8).public_key(),
        |p| p.request.challenge[0] ^= 1,
        |p| p.request.created_at_ms += 1,
        |p| p.epoch += 1,
        |p| p.issued_at_ms += 1,
        |p| p.expires_at_ms -= 1,
    ];
    for change in changes {
        let mut changed = proof.clone();
        change(&mut changed);
        // Match the mutated expected fields/epoch. Verification must still
        // reject the old signature, not pass only a structural comparison.
        let error = changed
            .verify(
                root.public_key(),
                &changed.request,
                changed.epoch,
                ISSUED + 2,
            )
            .err()
            .expect("tampered signed field accepted");
        assert!(error
            .to_string()
            .contains("invalid local authorization signature"));
    }
}

#[test]
fn different_roots_cannot_relabel_a_valid_proof() {
    let (_, _, mut proof) = proof();
    let impostor = signer(9);
    proof.root_public_key = impostor.public_key();
    proof.primary_key_ref = impostor.primary_key_ref();
    assert!(proof
        .verify(impostor.public_key(), &proof.request, EPOCH, ISSUED)
        .is_err());
}

#[test]
fn signature_tampering_wrong_domain_and_unsupported_version_are_rejected() {
    let (root, request, proof) = proof();
    let mut changed = proof.clone();
    changed.signature[0] ^= 1;
    assert!(changed
        .verify(root.public_key(), &request, EPOCH, ISSUED)
        .is_err());
    let mut changed = proof.clone();
    let mut wrong_domain = changed.signing_bytes();
    wrong_domain[0] ^= 1;
    changed.signature = root.signing_key.sign(&wrong_domain).to_bytes();
    assert!(changed
        .verify(root.public_key(), &request, EPOCH, ISSUED)
        .is_err());
    let mut changed = proof.clone();
    changed.version += 1;
    changed.signature = root.signing_key.sign(&changed.signing_bytes()).to_bytes();
    assert!(changed
        .verify(root.public_key(), &request, EPOCH, ISSUED)
        .is_err());
    let mut changed = proof;
    changed.primary_key_ref[0] ^= 1;
    changed.signature = root.signing_key.sign(&changed.signing_bytes()).to_bytes();
    assert!(changed
        .verify(root.public_key(), &request, EPOCH, ISSUED)
        .is_err());
}

#[test]
fn zero_bindings_weak_keys_and_root_subject_reuse_fail_before_signing() {
    assert!(LocalIdentitySignerV1::from_seed(Zeroizing::new([0; 32])).is_err());
    let root = signer(1);
    let changes: &[fn(&mut LocalKeyAuthorizationRequestV1)] = &[
        |r| r.purpose = [0; 32],
        |r| r.device_binding = [0; 32],
        |r| r.scope = [0; 32],
        |r| r.challenge = [0; 32],
        |r| r.subject_public_key = [0; 32],
        |r| r.created_at_ms = 0,
        |r| {
            r.subject_public_key = {
                let mut identity_point = [0; 32];
                identity_point[0] = 1;
                identity_point
            }
        },
    ];
    for change in changes {
        let mut request = request();
        change(&mut request);
        assert!(root
            .authorize(&request, EPOCH, ISSUED, ISSUED + 100)
            .is_err());
    }
    let mut request = request();
    request.subject_public_key = root.public_key();
    assert!(root
        .authorize(&request, EPOCH, ISSUED, ISSUED + 100)
        .is_err());
    let (_, request, proof) = proof();
    let mut weak_root = [0; 32];
    weak_root[0] = 1;
    assert!(proof.verify(weak_root, &request, EPOCH, ISSUED).is_err());
}

#[test]
fn mixed_order_and_noncanonical_public_keys_are_not_local_authorities_or_subjects() {
    let root = signer(1);
    let torsion = VerifyingKey::from_bytes(&[0; 32]).unwrap().to_edwards();
    let mixed = (signer(2).signing_key.verifying_key().to_edwards() + torsion)
        .compress()
        .to_bytes();
    assert!(!VerifyingKey::from_bytes(&mixed).unwrap().is_weak());
    let mut request = request();
    request.subject_public_key = mixed;
    assert!(root
        .authorize(&request, EPOCH, ISSUED, ISSUED + 100)
        .is_err());
    let (_, request, proof) = proof();
    assert!(proof.verify(mixed, &request, EPOCH, ISSUED).is_err());
    // Noncanonical y=p+1 encodes the identity under permissive decoding.
    let mut noncanonical = [0xff; 32];
    noncanonical[0] = 0xee;
    noncanonical[31] = 0x7f;
    assert!(validate_key(&noncanonical).is_err());
}

#[test]
fn issuance_and_verification_reject_invalid_time_windows_without_overflow() {
    let root = signer(1);
    let request = request();
    for (epoch, now, expiry) in [
        (0, ISSUED, ISSUED + 1),
        (EPOCH, CREATED - 1, ISSUED + 1),
        (
            EPOCH,
            CREATED + MAX_LOCAL_AUTHORIZATION_REQUEST_AGE_MS_V1 + 1,
            ISSUED + MAX_LOCAL_AUTHORIZATION_LIFETIME_MS_V1,
        ),
        (EPOCH, ISSUED, ISSUED),
        (EPOCH, ISSUED, ISSUED - 1),
        (
            EPOCH,
            ISSUED,
            ISSUED + MAX_LOCAL_AUTHORIZATION_LIFETIME_MS_V1 + 1,
        ),
        (EPOCH, ISSUED, u64::MAX),
    ] {
        assert!(root.authorize(&request, epoch, now, expiry).is_err());
    }
    let proof = root
        .authorize(&request, EPOCH, ISSUED, ISSUED + 100)
        .unwrap();
    assert!(proof
        .verify(root.public_key(), &request, EPOCH, ISSUED - 1)
        .is_err());
    assert!(proof
        .verify(root.public_key(), &request, EPOCH, u64::MAX)
        .is_err());
    let mut near_max = request;
    near_max.created_at_ms = u64::MAX - 2;
    let proof = root
        .authorize(&near_max, EPOCH, u64::MAX - 1, u64::MAX)
        .unwrap();
    assert!(proof
        .verify(root.public_key(), &near_max, EPOCH, u64::MAX - 1)
        .is_ok());
}

#[test]
fn independent_scopes_do_not_revoke_each_other_or_extend_original_proofs() {
    let (root, first, proof) = proof();
    let mut second = first.clone();
    second.scope[0] ^= 1;
    second.challenge[0] ^= 1;
    second.subject_public_key = signer(8).public_key();
    second.created_at_ms = ISSUED + 1;
    let second_proof = root
        .authorize(&second, EPOCH, ISSUED + 1, ISSUED + 100)
        .unwrap();
    assert!(proof
        .verify(root.public_key(), &first, EPOCH, ISSUED + 2)
        .is_ok());
    assert!(second_proof
        .verify(root.public_key(), &second, EPOCH, ISSUED + 2)
        .is_ok());
    assert!(proof
        .verify(root.public_key(), &second, EPOCH, ISSUED + 2)
        .is_err());
    assert!(second_proof
        .verify(root.public_key(), &first, EPOCH, ISSUED + 2)
        .is_err());
    assert_eq!(
        proof.expires_at_ms(),
        ISSUED + MAX_LOCAL_AUTHORIZATION_LIFETIME_MS_V1
    );
    // The owner supplies its latest persisted epoch after revocation. No prior
    // proof can claim that a different trusted epoch is still current.
    assert!(proof
        .verify(root.public_key(), &first, EPOCH + 1, ISSUED + 2)
        .is_err());
    assert!(second_proof
        .verify(root.public_key(), &second, EPOCH + 1, ISSUED + 2)
        .is_err());
}
