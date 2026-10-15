//! Exercise an explicitly selected existing mailbox host; never starts a server.
//! Input: fresh signed record, locally approved relay URL, DNS IP.
//! Ephemeral capabilities and plaintext remain in memory and are never logged.
use anyhow::{ensure, Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use ed25519_dalek::SigningKey;
use novovm_network::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::{IrohConfigV1, IrohEndpointOwnerV1},
    mailbox_rpc::{MailboxClientSessionV2, MailboxRejectionV1, MailboxReplyV1, MailboxRequestV1},
    mailbox_store::MailboxGrant,
    product_directory::{validate_relay_record_v1, PeerSignedRelayRecordV1},
    product_rendezvous_daemon::now_ms,
    product_service_lease::{request_mailbox_multiplex_v6, IrohServiceEndpointV1},
};
use rand::RngCore;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 4 || args.len() == 5,
        "usage: mailbox_peer_probe RECORD APPROVED_RELAY DNS_IP [IDLE_SECONDS]"
    );
    let idle_seconds: u64 = args.get(4).map(|v| v.parse()).transpose()?.unwrap_or(0);
    ensure!(
        idle_seconds <= 35,
        "idle probe exceeds bounded operation budget"
    );
    let bytes = std::fs::read(&args[1])?;
    ensure!(bytes.len() <= 16384, "record too large");
    let record: PeerSignedRelayRecordV1 = serde_json::from_slice(&bytes)?;
    validate_relay_record_v1(&record, now_ms()?)?;
    let route = IrohServiceEndpointV1::from_record(&record)?;
    ensure!(
        route.address.relays.iter().all(|r| r == &args[2]),
        "unapproved relay"
    );
    let pin = record.relay_public_key;
    let grant = MailboxGrant::generate()?;
    let mailbox = grant.id;
    let caps = Arc::new(Zeroizing::new((
        *grant.read.as_bytes(),
        *grant.write.as_bytes(),
    )));
    let object: [u8; 32] = rand::random();
    let key = Zeroizing::new(rand::random::<[u8; 32]>());
    let mut plain = Zeroizing::new([0u8; 64]);
    rand::rngs::OsRng.fill_bytes(plain.as_mut());
    let nonce: [u8; 12] = rand::random();
    let cipher = ChaCha20Poly1305::new_from_slice(key.as_ref()).unwrap();
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plain.as_ref())
        .map_err(|_| anyhow::anyhow!("encrypt probe"))?;
    let expires = (now_ms()? / 1000) as i64 + 300;
    // Sender and reader are independent transport owners and purpose keys.
    for receive in [false, true] {
        validate_relay_record_v1(&record, now_ms()?)?;
        let config = IrohConfigV1::new(&[args[2].clone()], args[3].parse()?)?;
        let client = IrohEndpointOwnerV1::bind(config, || Ok(())).await?;
        let signer_seed = Zeroizing::new(rand::random::<[u8; 32]>());
        let signer = SigningKey::from_bytes(&signer_seed);
        let address = route.address.clone();
        let caps = caps.clone();
        let expected = ciphertext.clone();
        let started = Instant::now();
        let result = tokio::time::timeout(Duration::from_secs(55), client.with_continuous_service_v4(
            || Ok(()), move |scope| Box::pin(async move {
                let connected = scope.connect(&address).await?;
                let path = connected.selected_path()?.to_string();
                let mut stream: CarrierStreamV1<'_> = connected.into();
                let _group = request_mailbox_multiplex_v6(&mut stream).await?;
                let mut session = MailboxClientSessionV2::begin_continuous_v6(&mut stream, &signer, pin).await?;
                println!("{}", serde_json::json!({"phase": if receive {"reader_authenticated"} else {"sender_authenticated"}, "elapsed_ms": started.elapsed().as_millis(), "path":path}));
                let operation = Instant::now();
                if !receive {
                    ensure!(matches!(session.request(MailboxRequestV1::Register { mailbox, read:caps.0, write:caps.1, expires }).await?, MailboxReplyV1::Registered), "register rejected");
                    if idle_seconds > 0 {
                        let idle = Instant::now();
                        tokio::time::sleep(Duration::from_secs(idle_seconds)).await;
                        println!("{}", serde_json::json!({"phase":"authenticated_idle_completed", "idle_ms":idle.elapsed().as_millis(), "application_requests":0}));
                    }
                    let put = || MailboxRequestV1::Put { mailbox, write:caps.1, object, ciphertext:expected.clone(), expires };
                    let MailboxReplyV1::Stored(receipt) = session.request(put()).await? else { anyhow::bail!("missing storage receipt") };
                    receipt.verify(pin, mailbox, object, &expected, expires)?;
                    println!("{}", serde_json::json!({"phase":"store_receipt_verified", "operation_ms":operation.elapsed().as_millis()}));
                    let MailboxReplyV1::Stored(duplicate) = session.request(put()).await? else { anyhow::bail!("duplicate put rejected") };
                    duplicate.verify(pin, mailbox, object, &expected, expires)?;
                    session.finish().await?;
                    Ok(None)
                } else {
                    ensure!(matches!(session.request(MailboxRequestV1::Fetch { mailbox, read:rand::random() }).await?, MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)), "wrong capability accepted");
                    let MailboxReplyV1::Object { object:actual, ciphertext, digest, expires:ttl } = session.request(MailboxRequestV1::Fetch { mailbox, read:caps.0 }).await? else { anyhow::bail!("missing ciphertext") };
                    ensure!(actual == object && ciphertext == expected && ttl == expires, "stored object changed");
                    ensure!(matches!(session.request(MailboxRequestV1::Consume { mailbox, read:caps.0, object, digest }).await?, MailboxReplyV1::Consumed), "consume rejected");
                    ensure!(matches!(session.request(MailboxRequestV1::Fetch { mailbox, read:caps.0 }).await?, MailboxReplyV1::Empty), "duplicate object remained");
                    println!("{}", serde_json::json!({"phase":"independent_fetch_consumed", "operation_ms":operation.elapsed().as_millis(), "wrong_capability_rejected":true, "original_ttl":true}));
                    session.finish().await?;
                    Ok(Some(ciphertext))
                }
            })
        )).await;
        client.close().await?;
        if let Some(bytes) = result.context("bounded probe expired")?? {
            let decrypted = Zeroizing::new(
                cipher
                    .decrypt(Nonce::from_slice(&nonce), bytes.as_slice())
                    .map_err(|_| anyhow::anyhow!("decrypt fetched probe"))?,
            );
            ensure!(decrypted.as_slice() == plain.as_ref(), "plaintext mismatch");
        }
    }
    println!(
        "{}",
        serde_json::json!({"result":"PASS", "device_delivery_receipt":false, "billing_exercised":false})
    );
    Ok(())
}
