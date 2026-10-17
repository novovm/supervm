use super::*;
use crate::{
    mailbox_store::MailboxStore,
    product_directory::{
        sign_relay_record_v1, validate_relay_record_v1, PeerSignedRelayRecordV1, RelayEndpointV1,
        RelayTransportV1,
    },
    product_service_lease::{
        IrohServiceEndpointV1, NativeCarrierMailboxEndpointV7, NativeMailboxEndpointV7,
    },
};
use ed25519_dalek::SigningKey;

const CONTEXT: [u8; 32] = [71; 32];
fn now() -> Result<u64> {
    crate::product_rendezvous_daemon::now_ms()
}
fn record(
    key: &SigningKey,
    issued: u64,
    expires: u64,
    seq: u64,
) -> Result<PeerSignedRelayRecordV1> {
    Ok(sign_relay_record_v1(
        key,
        "native-locator",
        vec![NativeMailboxEndpointV7 {
            address: "127.0.0.1:4433".parse()?,
            max_connections: 8,
            max_bytes_per_minute: 8 * 1024 * 1024,
        }
        .endpoint()?],
        issued,
        expires,
        seq,
    )?)
}
#[test]
fn locator_apfl_preserves_original_signature_and_every_field() -> Result<()> {
    let key = SigningKey::from_bytes(&[72; 32]);
    let now = now()?;
    let signed = record(&key, now, now + 60_000, 42)?;
    let request = encode_requests(CONTEXT, &[Request::ServiceRecordV7])?;
    let batch = RequestBatch::decode(&request, CONTEXT)?;
    assert!(matches!(batch.request(0)?, Request::ServiceRecordV7));
    assert!(!batch.has_waiting_watch()?);
    let wire = encode_replies(
        CONTEXT,
        &[Reply::ServiceRecordV7 {
            record: signed.clone(),
        }],
    )?;
    let replies = decode_replies(&wire, CONTEXT, 1)?;
    let Reply::ServiceRecordV7 { record: decoded } = &replies[0] else {
        anyhow::bail!("wrong reply")
    };
    assert_eq!(*decoded, signed);
    validate_relay_record_v1(decoded, now)?;
    assert!(!wire
        .windows(b"relay_public_key".len())
        .any(|v| v == b"relay_public_key"));
    assert!(serde_json::from_slice::<serde_json::Value>(&wire).is_err());
    Ok(())
}
#[test]
fn locator_codec_preserves_all_existing_transport_tags() -> Result<()> {
    let key = SigningKey::from_bytes(&[72; 32]);
    let now = now()?;
    for (transport, uri) in [
        (RelayTransportV1::Wss443, "wss://example.invalid:443/"),
        (RelayTransportV1::Quic443, "quic://example.invalid:443"),
        (
            RelayTransportV1::IrohRendezvousV1,
            "iroh-rendezvous-v1://existing",
        ),
        (RelayTransportV1::TlsTcpV1, "tls-service-v1://existing"),
        (
            RelayTransportV1::Udp,
            "udp://127.0.0.1:4433/novonet-mailbox/v7",
        ),
    ] {
        let signed = sign_relay_record_v1(
            &key,
            "all-fields",
            vec![RelayEndpointV1 {
                transport,
                uri: uri.into(),
                priority: u16::MAX,
                max_sessions: u32::MAX,
                max_bytes_per_minute: u64::MAX,
            }],
            now,
            now + 60_000,
            u64::MAX,
        )?;
        let decoded = locator::decode(&locator::encode(&signed)?)?;
        assert_eq!(decoded, signed);
        validate_relay_record_v1(&decoded, now)?;
    }
    Ok(())
}
#[test]
fn locator_codec_rejects_truncation_overlong_values_and_unknown_tags() -> Result<()> {
    let key = SigningKey::from_bytes(&[72; 32]);
    let now = now()?;
    let signed = record(&key, now, now + 60_000, 42)?;
    let bytes = locator::encode(&signed)?;
    for length in 0..bytes.len() {
        assert!(locator::decode(&bytes[..length]).is_err());
    }
    let mut changed = bytes.clone();
    changed.push(0);
    assert!(locator::decode(&changed).is_err());
    changed = bytes.clone();
    changed[0] = 2;
    assert!(locator::decode(&changed).is_err());
    changed = bytes.clone();
    changed[2..4].copy_from_slice(&257u16.to_le_bytes());
    assert!(locator::decode(&changed).is_err());
    let count_at = 2 + 2 + signed.record_id.len() + 2 + signed.relay_peer_id.len() + 32;
    changed = bytes.clone();
    changed[count_at] = 4;
    assert!(locator::decode(&changed).is_err());
    changed = bytes.clone();
    changed[count_at + 1] = 0;
    assert!(locator::decode(&changed).is_err());
    changed = bytes;
    changed[count_at + 2..count_at + 4].copy_from_slice(&2049u16.to_le_bytes());
    assert!(locator::decode(&changed).is_err());
    assert!(locator::decode(&vec![0; 8193]).is_err());
    for which in 0..5 {
        let mut invalid = signed.clone();
        match which {
            0 => invalid.record_id = "x".repeat(257),
            1 => invalid.relay_peer_id = "x".repeat(129),
            2 => invalid.endpoints[0].uri = "x".repeat(2049),
            3 => invalid.endpoints = vec![invalid.endpoints[0].clone(); 4],
            _ => invalid.signature.push(0),
        }
        assert!(locator::encode(&invalid).is_err());
    }
    Ok(())
}
#[test]
fn locator_apfl_rejects_wrong_schema_context_and_extra_fields() -> Result<()> {
    let key = SigningKey::from_bytes(&[72; 32]);
    let now = now()?;
    let signed = record(&key, now, now + 60_000, 42)?;
    let encoded = locator::encode(&signed)?;
    let fields: Fields<'_> = [&[11], &encoded, &[], &[], &[], &[], &[], &[]];
    let wrong_schema = build(REQUEST_SCHEMA, CONTEXT, &[fields])?;
    assert!(decode_replies(&wrong_schema, CONTEXT, 1).is_err());
    let wire = build(REPLY_SCHEMA, CONTEXT, &[fields])?;
    assert!(decode_replies(&wire, [72; 32], 1).is_err());
    assert!(decode_replies(&wire, CONTEXT, 2).is_err());
    let mut extra = fields;
    extra[7] = &[1];
    assert!(decode_replies(&build(REPLY_SCHEMA, CONTEXT, &[extra])?, CONTEXT, 1).is_err());
    let request: Fields<'_> = [&[9], &[1], &[], &[], &[], &[], &[], &[]];
    assert!(RequestBatch::decode(&build(REQUEST_SCHEMA, CONTEXT, &[request])?, CONTEXT).is_err());
    Ok(())
}
#[test]
fn locator_is_rejected_by_legacy_json_in_both_directions() -> Result<()> {
    let key = SigningKey::from_bytes(&[72; 32]);
    let now = now()?;
    let signed = record(&key, now, now + 60_000, 42)?;
    assert!(serde_json::to_vec(&Request::ServiceRecordV7).is_err());
    assert!(serde_json::from_str::<Request>(r#""ServiceRecordV7""#).is_err());
    assert!(serde_json::to_vec(&Reply::ServiceRecordV7 {
        record: signed.clone()
    })
    .is_err());
    let legacy = serde_json::json!({"ServiceRecordV7": {"record": signed}});
    assert!(serde_json::from_value::<Reply>(legacy).is_err());
    Ok(())
}
#[tokio::test]
async fn locator_owner_snapshot_rejects_wrong_identity_signature_stale_and_rollback() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let key = SigningKey::from_bytes(&[72; 32]);
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(
            directory.path().join("mailbox.db"),
            Default::default(),
            true,
        )?,
        key.clone(),
    );
    assert!(matches!(
        service.request_local_v1(Request::ServiceRecordV7).await?,
        Reply::Rejected(MailboxRejectionV1::Invalid)
    ));
    let now = now()?;
    let signed = record(&key, now - 10, now + 60_000, 42)?;
    service.update_native_record_v7(signed.clone())?;
    service.update_native_record_v7(signed.clone())?;
    let mut bad_signature = signed.clone();
    bad_signature.signature[0] ^= 1;
    assert!(service.update_native_record_v7(bad_signature).is_err());
    assert!(service
        .update_native_record_v7(record(
            &SigningKey::from_bytes(&[73; 32]),
            now,
            now + 60_000,
            43
        )?)
        .is_err());
    assert!(service
        .update_native_record_v7(record(&key, now - 100, now - 1, 43)?)
        .is_err());
    assert!(service
        .update_native_record_v7(record(&key, now + 60_000, now + 120_000, 43)?)
        .is_err());
    assert!(service
        .update_native_record_v7(record(&key, now - 10, now + 60_000, 41)?)
        .is_err());
    assert!(service
        .update_native_record_v7(record(&key, now - 11, now + 60_000, 43)?)
        .is_err());
    assert!(service
        .update_native_record_v7(record(&key, now - 10, now + 60_001, 42)?)
        .is_err());
    let no_native = sign_relay_record_v1(
        &key,
        "missing-native",
        vec![RelayEndpointV1 {
            transport: RelayTransportV1::Wss443,
            uri: "wss://example.invalid:443/".into(),
            priority: 0,
            max_sessions: 8,
            max_bytes_per_minute: 8 * 1024 * 1024,
        }],
        now,
        now + 60_000,
        43,
    )?;
    assert!(service.update_native_record_v7(no_native).is_err());
    let Reply::ServiceRecordV7 { record: actual } =
        service.request_local_v1(Request::ServiceRecordV7).await?
    else {
        anyhow::bail!("snapshot lost")
    };
    assert_eq!(actual, signed);
    let newer = record(&key, now, now + 60_000, 43)?;
    service.update_native_record_v7(newer.clone())?;
    let request = encode_requests(CONTEXT, &[Request::ServiceRecordV7])?;
    let response = execute_local_batch(&service, &request, CONTEXT, || Ok(())).await?;
    let replies = decode_replies(&response, CONTEXT, 1)?;
    let Reply::ServiceRecordV7 { record: actual } = &replies[0] else {
        anyhow::bail!("wrong local reply")
    };
    assert_eq!(actual, &newer);
    Ok(())
}
#[tokio::test]
async fn locator_owner_does_not_serve_an_expired_snapshot() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let key = SigningKey::from_bytes(&[72; 32]);
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(
            directory.path().join("mailbox.db"),
            Default::default(),
            true,
        )?,
        key.clone(),
    );
    let issued = now()?;
    service.update_native_record_v7(record(&key, issued, issued + 1_000, 1)?)?;
    tokio::time::sleep(std::time::Duration::from_millis(1_050)).await;
    assert!(matches!(
        service.request_local_v1(Request::ServiceRecordV7).await?,
        Reply::Rejected(MailboxRejectionV1::Invalid)
    ));
    Ok(())
}

#[tokio::test]
async fn locator_owner_accepts_explicit_native_carrier_and_rejects_legacy_or_invalid_updates(
) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let key = SigningKey::from_bytes(&[72; 32]);
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(
            directory.path().join("mailbox.db"),
            Default::default(),
            true,
        )?,
        key.clone(),
    );
    let carrier = NativeCarrierMailboxEndpointV7 {
        iroh: IrohServiceEndpointV1 {
            instance: [7; 32],
            address: crate::iroh_transport::IrohAddressV1 {
                endpoint_key: SigningKey::from_bytes(&[73; 32]).verifying_key().to_bytes(),
                direct: vec!["127.0.0.1:4434".parse()?],
                relays: Vec::new(),
            },
        },
    };
    let now = now()?;
    let make = |signer: &SigningKey, endpoints, issued, expires, sequence| {
        sign_relay_record_v1(
            signer,
            "carrier-locator",
            endpoints,
            issued,
            expires,
            sequence,
        )
    };
    let signed = make(&key, vec![carrier.endpoint()?], now - 10, now + 60_000, 42)?;
    service.update_native_record_v7(signed.clone())?;
    service.update_native_record_v7(signed.clone())?;
    let mut forged = signed.clone();
    forged.signature[0] ^= 1;
    let mut malformed = carrier.endpoint()?;
    malformed.uri.push('/');
    // Even an otherwise valid UDP-native alternative cannot smuggle this
    // malformed service URI into the retained native locator snapshot.
    let udp = record(&key, now, now + 60_000, 43)?.endpoints.remove(0);
    for rejected in [
        forged,
        make(
            &SigningKey::from_bytes(&[74; 32]),
            vec![carrier.endpoint()?],
            now,
            now + 60_000,
            43,
        )?,
        make(&key, vec![carrier.iroh.endpoint()?], now, now + 60_000, 43)?,
        make(&key, vec![udp, malformed], now, now + 60_000, 43)?,
        make(&key, vec![carrier.endpoint()?], now - 100, now - 1, 43)?,
        make(
            &key,
            vec![carrier.endpoint()?],
            now + 60_000,
            now + 120_000,
            43,
        )?,
        make(&key, vec![carrier.endpoint()?], now, now + 60_000, 41)?,
        make(&key, vec![carrier.endpoint()?], now - 11, now + 60_000, 43)?,
        make(&key, vec![carrier.endpoint()?], now - 10, now + 60_001, 42)?,
    ] {
        assert!(service.update_native_record_v7(rejected).is_err());
        let Reply::ServiceRecordV7 { record: actual } =
            service.request_local_v1(Request::ServiceRecordV7).await?
        else {
            anyhow::bail!("carrier snapshot lost after rejected update")
        };
        assert_eq!(actual, signed);
    }
    let newer = make(&key, vec![carrier.endpoint()?], now, now + 60_000, 43)?;
    service.update_native_record_v7(newer.clone())?;
    let request = encode_requests(CONTEXT, &[Request::ServiceRecordV7])?;
    let response = execute_local_batch(&service, &request, CONTEXT, || Ok(())).await?;
    let replies = decode_replies(&response, CONTEXT, 1)?;
    let Reply::ServiceRecordV7 { record: actual } = &replies[0] else {
        anyhow::bail!("wrong carrier locator reply")
    };
    assert_eq!(actual, &newer);
    Ok(())
}
