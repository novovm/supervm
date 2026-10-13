#![cfg(feature = "mailbox-seal")]
use novovm_network::mailbox_seal::*;

#[test]
fn recipient_only_randomized_replica_bound_and_tamper_rejected() {
    let secret = MailboxSecretV1::generate().unwrap();
    let wrong = MailboxSecretV1::generate().unwrap();
    let body = b"explicit HPKE test fixture, not a real user message";
    let first = seal_mailbox_v1(secret.public_key(), body, b"replica-one").unwrap();
    let second = seal_mailbox_v1(secret.public_key(), body, b"replica-two").unwrap();
    assert_ne!(first, second);
    assert_eq!(
        secret.open(&first, b"replica-one").unwrap().as_slice(),
        body
    );
    assert!(wrong.open(&first, b"replica-one").is_err());
    assert!(secret.open(&first, b"replica-two").is_err());
    let restored = MailboxSecretV1::restore(secret.protected_seed()).unwrap();
    assert_eq!(restored.public_key(), secret.public_key());
    assert_eq!(
        restored.open(&second, b"replica-two").unwrap().as_slice(),
        body
    );
    for position in [0, 8, 40, first.len() - 1] {
        let mut corrupt = first.clone();
        corrupt[position] ^= 1;
        assert!(secret.open(&corrupt, b"replica-one").is_err());
    }
    assert!(seal_mailbox_v1([0; 32], body, b"replica").is_err());
    assert!(seal_mailbox_v1(secret.public_key(), &vec![1; MAX_PLAINTEXT + 1], b"replica").is_err());
}
