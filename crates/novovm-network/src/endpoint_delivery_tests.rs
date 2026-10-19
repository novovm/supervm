use super::*;
use crate::{
    mailbox_rpc::{
        MailboxRejectionV1 as Rejection, MailboxReplyV1 as Reply, MailboxRequestV1 as Request,
        MailboxServiceV1,
    },
    mailbox_store::{Limits, MailboxStore},
};
use ed25519_dalek::SigningKey;
use std::sync::atomic::AtomicUsize;

fn request(write: u8) -> Request {
    Request::EndpointDeliverV1 {
        mailbox: [1; 32],
        write: [write; 32],
        object: [3; 32],
        ciphertext: vec![7; 24],
        expires: 100,
    }
}
fn handler(calls: Arc<AtomicUsize>) -> EndpointDeliveryHandlerV1 {
    Arc::new(move |request| {
        calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            ensure!(
                request.mailbox == [1; 32]
                    && request.object == [3; 32]
                    && request.ciphertext == vec![7; 24]
                    && request.expires == 100
            );
            Ok(EndpointDeliveryResultV1::Delivered {
                receipt_object: [4; 32],
                receipt_expires: 200,
                receipt: vec![8; 32],
            })
        })
    })
}

#[tokio::test]
async fn endpoint_routes_are_capability_checked_generation_owned_and_never_write_sqlite(
) -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("mailbox.db");
    let service = MailboxServiceV1::new(
        MailboxStore::open(&path, Limits::default())?,
        SigningKey::from_bytes(&[9; 32]),
    );
    let sql = rusqlite::Connection::open(&path)?;
    let names = sql
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (i, name) in names.iter().enumerate() {
        for action in ["INSERT", "UPDATE", "DELETE"] {
            sql.execute_batch(&format!("CREATE TRIGGER forbid_{i}_{action} BEFORE {action} ON \"{}\" BEGIN SELECT RAISE(ABORT, 'endpoint touched custody database'); END;", name.replace('"', "\"\"")))?;
        }
    }
    assert!(matches!(
        service.request_local_v1(request(2)).await?,
        Reply::Rejected(Rejection::Unavailable)
    ));
    let calls = Arc::new(AtomicUsize::new(0));
    let old = service.register_endpoint_delivery_v1([1; 32], [2; 32], handler(calls.clone()))?;
    assert!(matches!(
        service.request_local_v1(request(99)).await?,
        Reply::Rejected(Rejection::Unauthorized)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let current =
        service.register_endpoint_delivery_v1([1; 32], [2; 32], handler(calls.clone()))?;
    drop(old);
    assert!(
        matches!(service.request_local_v1(request(2)).await?, Reply::EndpointDeliveredV1 { object, receipt_object, receipt_expires:200, receipt } if object == [3;32] && receipt_object == [4;32] && receipt == vec![8;32])
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(current);
    assert!(matches!(
        service.request_local_v1(request(2)).await?,
        Reply::Rejected(Rejection::Unavailable)
    ));
    Ok(())
}

#[tokio::test]
async fn withdrawal_during_callback_does_not_emit_success_and_callback_failures_propagate(
) -> Result<()> {
    let registry = Arc::new(EndpointDeliveryRegistryV1::default());
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let ready = started.clone();
    let done = release.clone();
    let guard = registry.register(
        [1; 32],
        [2; 32],
        Arc::new(move |_| {
            let ready = ready.clone();
            let done = done.clone();
            Box::pin(async move {
                ready.notify_one();
                done.notified().await;
                Ok(EndpointDeliveryResultV1::Delivered {
                    receipt_object: [4; 32],
                    receipt_expires: 200,
                    receipt: vec![8],
                })
            })
        }),
    )?;
    let worker = registry.clone();
    let running = tokio::spawn(async move {
        worker
            .deliver(
                &[2; 32],
                EndpointDeliveryRequestV1 {
                    mailbox: [1; 32],
                    object: [3; 32],
                    ciphertext: vec![7],
                    expires: 100,
                },
            )
            .await
    });
    started.notified().await;
    drop(guard);
    release.notify_one();
    assert!(matches!(
        running.await??,
        EndpointDeliveryResultV1::Unavailable
    ));
    let _guard = registry.register(
        [1; 32],
        [2; 32],
        Arc::new(|_| Box::pin(async { anyhow::bail!("application storage failed") })),
    )?;
    assert!(registry
        .deliver(
            &[2; 32],
            EndpointDeliveryRequestV1 {
                mailbox: [1; 32],
                object: [3; 32],
                ciphertext: vec![7],
                expires: 100
            }
        )
        .await
        .is_err());
    Ok(())
}

#[test]
fn endpoint_registration_is_bounded_and_dropping_frees_capacity() -> Result<()> {
    let registry = EndpointDeliveryRegistryV1::default();
    let mut guards = Vec::new();
    for i in 1u32..=256 {
        let mut mailbox = [0; 32];
        mailbox[..4].copy_from_slice(&i.to_le_bytes());
        guards.push(registry.register(mailbox, [2; 32], handler(Arc::new(AtomicUsize::new(0))))?);
    }
    assert!(registry
        .register([99; 32], [2; 32], handler(Arc::new(AtomicUsize::new(0))))
        .is_err());
    guards.pop();
    let _next = registry.register([99; 32], [2; 32], handler(Arc::new(AtomicUsize::new(0))))?;
    Ok(())
}
