use super::*;

fn intent() -> ApplicationGrantIntentV1 {
    ApplicationGrantIntentV1 {
        selector: ApplicationGrantSelectorV1 {
            chain_id: 7,
            genesis: [1; 32],
            identity: UcaIdentityIdV1::new("uca:alice").unwrap(),
            application: [2; 32],
            service: [3; 32],
            device: [4; 32],
        },
        issuer_key_ref: [5; 32],
        expected_revision: 0,
        valid_from_height: 10,
        valid_until_height: 20,
        action: ApplicationGrantActionV1::Authorize {
            not_before_height: 10,
            expires_at_height: 100,
        },
    }
}
fn signed() -> SignedApplicationGrantV1 {
    SignedApplicationGrantV1 {
        intent: intent(),
        public_key: vec![1; MLDSA65_PUBLIC_KEY_BYTES],
        signature: vec![2; MLDSA65_SIGNATURE_BYTES],
    }
}

#[test]
fn observed_grant_rejects_superseded_sessions_and_revoked_devices() {
    use crate::native_query::{ApplicationGrantViewV1, IdentityStatusV1};

    let selector = intent().selector;
    let mut view = ApplicationGrantViewV1 {
        state_root: [7; 32],
        height: 10,
        revision: 8,
        issuer_key_ref: [5; 32],
        identity_key_ref: [5; 32],
        identity_status: IdentityStatusV1::Active,
        revoked: false,
        not_before_height: 10,
        expires_at_height: 20,
    };
    assert!(view.permits_device_at_observed_height(&selector, 8));
    assert!(!view.permits_device_at_observed_height(&selector, 7));
    view.revoked = true;
    assert!(!view.permits_device_at_observed_height(&selector, 8));
    view.revoked = false;
    view.height = 20;
    assert!(!view.permits_device_at_observed_height(&selector, 8));
}

#[test]
fn canonical_application_authority_roundtrip_is_distinct_from_old_ed25519() {
    for action in [intent().action, ApplicationGrantActionV1::Revoke] {
        let mut signed = signed();
        signed.intent.action = action;
        let bytes = signed.encode().unwrap();
        assert_eq!(SignedApplicationGrantV1::decode(&bytes).unwrap(), signed);
        assert!(bytes.starts_with(MAGIC));
        let mut short = signed;
        short.signature.truncate(64);
        assert!(short.encode().is_err());
    }
}

#[test]
fn application_authority_rejects_truncation_suffix_wrong_magic_and_excessive_lengths() {
    let bytes = signed().encode().unwrap();
    for length in 0..bytes.len() {
        assert!(SignedApplicationGrantV1::decode(&bytes[..length]).is_err());
    }
    let mut suffix = bytes.clone();
    suffix.push(0);
    assert!(SignedApplicationGrantV1::decode(&suffix).is_err());
    for offset in [0, 4, 5, 6, 7] {
        let mut bad = bytes.clone();
        bad[offset] ^= 1;
        assert!(SignedApplicationGrantV1::decode(&bad).is_err());
    }
    let mut huge = bytes;
    huge[6..8].copy_from_slice(&u16::MAX.to_be_bytes());
    assert!(SignedApplicationGrantV1::decode(&huge).is_err());
}

#[test]
fn every_selector_field_separates_the_grant_and_signature_domains() {
    let original = intent();
    let id = original.selector.grant_id().unwrap();
    for case in 0..6 {
        let mut other = original.clone();
        match case {
            0 => other.selector.chain_id += 1,
            1 => other.selector.genesis[0] ^= 1,
            2 => other.selector.identity = UcaIdentityIdV1::new("uca:bob").unwrap(),
            3 => other.selector.application[0] ^= 1,
            4 => other.selector.service[0] ^= 1,
            5 => other.selector.device[0] ^= 1,
            _ => unreachable!(),
        }
        assert_ne!(other.selector.grant_id().unwrap(), id);
        assert_ne!(
            other.signing_bytes().unwrap(),
            original.signing_bytes().unwrap()
        );
    }
}

#[test]
fn invalid_domains_windows_and_revision_overflow_are_rejected() {
    for case in 0..11 {
        let mut i = intent();
        match case {
            0 => i.selector.chain_id = 0,
            1 => i.selector.genesis = [0; 32],
            2 => i.selector.application = [0; 32],
            3 => i.selector.service = [0; 32],
            4 => i.selector.device = [0; 32],
            5 => i.issuer_key_ref = [0; 32],
            6 => i.expected_revision = u64::MAX,
            7 => i.valid_until_height = i.valid_from_height,
            8 => i.valid_until_height = i.valid_from_height + MAX_COMMAND_WINDOW + 1,
            9 => {
                i.action = ApplicationGrantActionV1::Authorize {
                    not_before_height: 9,
                    expires_at_height: 10,
                }
            }
            10 => i.expected_revision = u64::MAX - 1,
            _ => unreachable!(),
        }
        assert!(i.signing_bytes().is_err(), "case {case}");
    }
    let mut last_revoke = intent();
    last_revoke.expected_revision = u64::MAX - 1;
    last_revoke.action = ApplicationGrantActionV1::Revoke;
    assert!(last_revoke.signing_bytes().is_ok());
}
