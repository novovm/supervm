use super::*;
fn request(size: usize) -> Request {
    Request::EndpointDeliverV1 {
        mailbox: [1; 32],
        write: [2; 32],
        object: [3; 32],
        ciphertext: vec![4; size],
        expires: 100,
    }
}
fn reply(size: usize) -> Reply {
    Reply::EndpointDeliveredV1 {
        object: [3; 32],
        receipt_object: [5; 32],
        receipt_expires: 101,
        receipt: vec![6; size],
    }
}
#[test]
fn endpoint_codec_is_small_native_only_and_preserves_all_bindings() -> Result<()> {
    let context = [7; 32];
    let bytes = encode_requests(context, &[request(64)])?;
    assert_eq!(
        wire_schema(&bytes, REQUEST_SCHEMA, BULK_REQUEST_SCHEMA)?,
        REQUEST_SCHEMA
    );
    let batch = RequestBatch::decode(&bytes, context)?;
    assert!(
        matches!(batch.request(0)?, Request::EndpointDeliverV1 { mailbox, write, object, ref ciphertext, expires:100 } if mailbox == [1;32] && write == [2;32] && object == [3;32] && ciphertext == &vec![4;64])
    );
    assert!(!batch.has_waiting_watch()?);
    assert!(RequestBatch::decode(&bytes, [8; 32]).is_err());
    assert!(serde_json::to_vec(&request(1)).is_err());
    assert!(serde_json::to_vec(&reply(1)).is_err());
    let encoded = encode_replies(context, &[reply(32)])?;
    assert!(
        matches!(decode_replies(&encoded, context, 1)?.pop().unwrap(), Reply::EndpointDeliveredV1 { object, receipt_object, receipt_expires:101, receipt } if object == [3;32] && receipt_object == [5;32] && receipt == vec![6;32])
    );
    for size in [0, 65_537] {
        assert!(encode_requests(context, &[request(size)]).is_err());
        assert!(encode_replies(context, &[reply(size)]).is_err());
    }
    assert!(encode_replies(
        context,
        &[
            reply(1),
            Reply::BulkReady {
                max_ciphertext: crate::mailbox_store::MAX_BULK_CIPHERTEXT as u32
            }
        ]
    )
    .is_err());
    let tags = [255];
    let empty: Fields<'_> = [&tags, &[], &[], &[], &[], &[], &[], &[]];
    assert!(validate_request(&empty).is_err());
    assert!(validate_reply_bound(&empty, 65_536).is_err());
    Ok(())
}
