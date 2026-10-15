//! Original executable, Windows DPAPI and SQLite, isolated owner fixture.
use super::*;
use novovm_network::{
    carrier_stream::CarrierStreamV1,
    mailbox_rpc::{MailboxClientSessionV2, MailboxReplyV1, MailboxRequestV1},
    mailbox_store::MailboxGrant,
    tls_transport::TlsScopeV1,
};

async fn configure(
    f: &Fixture,
    command: &str,
    value: &std::ffi::OsStr,
    success: bool,
) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let mut child = Process(
        Command::new(env!("CARGO_BIN_EXE_novovm-rendezvous-service"))
            .arg(command)
            .arg(f.0.join("owned.json"))
            .arg(value)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000)
            .spawn()?,
    );
    child.finish(success).await
}

async fn exchange_tls(
    record: &PeerSignedRelayRecordV1,
    grant: &MailboxGrant,
    expires: i64,
    put: bool,
) -> Result<()> {
    let mut history = ServiceLeaseBookV1::new_self_owned(
        SelfOwnedTrustV1 {
            version: 1,
            node_pins: [record.relay_public_key].into(),
        },
        vec![record.clone()],
        now_ms()?,
    )?;
    let route = TlsServiceEndpointV1::from_record(record)?.context("TLS route missing")?;
    history.check_tls_route_v1(record.relay_public_key, &route, now_ms()?)?;
    let scope = TlsScopeV1::new_continuous_v4(|| Ok(()))?;
    let tcp = tokio::net::TcpStream::connect(route.address).await?;
    tcp.set_nodelay(true)?;
    let mut stream: CarrierStreamV1<'_> = scope.connect(tcp, route.endpoint_key).await?.into();
    request_mailbox_continuous_v6(&mut stream).await?;
    let identity = key(73);
    let mut session = MailboxClientSessionV2::begin_continuous_v6(
        &mut stream,
        &identity,
        record.relay_public_key,
    )
    .await?;
    let object = [9; 32];
    let data = vec![43; 128];
    if put {
        ensure!(matches!(
            session
                .request(MailboxRequestV1::Register {
                    mailbox: grant.id,
                    write: *grant.write.as_bytes(),
                    read: *grant.read.as_bytes(),
                    expires,
                })
                .await?,
            MailboxReplyV1::Registered
        ));
        let MailboxReplyV1::Stored(receipt) = session
            .request(MailboxRequestV1::Put {
                mailbox: grant.id,
                write: *grant.write.as_bytes(),
                object,
                ciphertext: data.clone(),
                expires,
            })
            .await?
        else {
            anyhow::bail!("no durable receipt")
        };
        receipt.verify(record.relay_public_key, grant.id, object, &data, expires)?;
    } else {
        let MailboxReplyV1::Object {
            object: found,
            ciphertext,
            digest,
            expires: original,
        } = session
            .request(MailboxRequestV1::Fetch {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
            })
            .await?
        else {
            anyhow::bail!("persisted object missing")
        };
        ensure!(
            found == object && ciphertext == data && original == expires,
            "custody altered"
        );
        ensure!(matches!(
            session
                .request(MailboxRequestV1::Consume {
                    mailbox: grant.id,
                    read: *grant.read.as_bytes(),
                    object,
                    digest,
                })
                .await?,
            MailboxReplyV1::Consumed
        ));
    }
    session.finish().await
}

#[tokio::test(flavor = "current_thread")]
async fn windows_tls_policy_restart_preserves_identity_and_real_custody() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let fixture = Fixture::new()?;
        fixture.owned_config()?;
        let first_process = fixture.launch_owned("--provision-self-owned")?;
        let first = fixture.owned_record(0).await?;
        ensure!(TlsServiceEndpointV1::from_record(&first)?.is_none());
        let tls_file = fixture.0.join("tls.json");
        std::fs::write(
            &tls_file,
            br#"{"version":1,"listen":"127.0.0.1:0","advertised":"127.0.0.1:0","max_connections":8}"#,
        )?;
        let policy_path = fixture.0.join("self-owned-policy.dpapi");
        let original_policy = std::fs::read(&policy_path)?;
        // No live mutation, and failed commands do not rewrite protected bytes.
        configure(
            &fixture,
            "--set-self-owned-tls",
            tls_file.as_os_str(),
            false,
        )
        .await?;
        ensure!(std::fs::read(&policy_path)? == original_policy);
        drop(first_process);
        let keys = std::fs::read(fixture.0.join("keys.dpapi"))?;
        let journal = std::fs::read(fixture.0.join("service.journal"))?;
        configure(&fixture, "--set-self-owned-tls", tls_file.as_os_str(), true).await?;
        ensure!(std::fs::read(fixture.0.join("service.journal"))? == journal);
        let enabled_policy = std::fs::read(&policy_path)?;
        ensure!(enabled_policy != original_policy);
        // No implicit direct route under a relay-only choice.
        configure(
            &fixture,
            "--set-self-owned-carrier",
            "relay-only".as_ref(),
            false,
        )
        .await?;
        ensure!(std::fs::read(&policy_path)? == enabled_policy);
        let second_process = fixture.launch_owned("--foreground")?;
        let second = fixture.owned_record(first.sequence).await?;
        let second_tls = TlsServiceEndpointV1::from_record(&second)?.unwrap();
        ensure!(second_tls.max_connections == 8);
        ensure!(second.relay_public_key == first.relay_public_key);
        ensure!(second_tls.endpoint_key != second.relay_public_key);
        ensure!(
            second_tls.endpoint_key
                != IrohServiceEndpointV1::from_record(&second)?
                    .address
                    .endpoint_key
        );
        let grant = MailboxGrant::generate()?;
        let expires = (now_ms()? / 1000) as i64 + 300;
        exchange_tls(&second, &grant, expires, true).await?;
        drop(second_process); // Real abrupt process exit after durable StoreReceipt.
        let third_process = fixture.launch_owned("--foreground")?;
        let third = fixture.owned_record(second.sequence).await?;
        ensure!(
            TlsServiceEndpointV1::from_record(&third)?
                .unwrap()
                .endpoint_key
                == second_tls.endpoint_key
        );
        ensure!(third.relay_public_key == first.relay_public_key);
        ensure!(
            IrohServiceEndpointV1::from_record(&third)?
                .address
                .endpoint_key
                == IrohServiceEndpointV1::from_record(&first)?
                    .address
                    .endpoint_key
        );
        exchange_tls(&third, &grant, expires, false).await?;
        drop(third_process);
        let mailbox = std::fs::read(fixture.0.join("mailbox.sqlite"))?;
        configure(&fixture, "--set-self-owned-tls", "off".as_ref(), true).await?;
        ensure!(std::fs::read(fixture.0.join("mailbox.sqlite"))? == mailbox);
        let _fourth_process = fixture.launch_owned("--foreground")?;
        let fourth = fixture.owned_record(third.sequence).await?;
        ensure!(TlsServiceEndpointV1::from_record(&fourth)?.is_none());
        ensure!(fourth.relay_public_key == first.relay_public_key);
        ensure!(std::fs::read(fixture.0.join("keys.dpapi"))? == keys);
        Ok(())
    })
    .await?
}

#[tokio::test(flavor = "current_thread")]
async fn windows_tls_invalid_settings_never_change_custody() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.owned_config()?;
    let process = fixture.launch_owned("--provision-self-owned")?;
    fixture.owned_record(0).await?;
    drop(process);
    let policy_path = fixture.0.join("self-owned-policy.dpapi");
    let before = std::fs::read(&policy_path)?;
    let path = fixture.0.join("tls.json");
    for invalid in [
        r#"{"version":2,"listen":"127.0.0.1:0","advertised":"127.0.0.1:0"}"#,
        r#"{"version":1,"listen":"127.0.0.1:0","advertised":"0.0.0.0:443"}"#,
        r#"{"version":1,"listen":"127.0.0.1:0","advertised":"224.0.0.1:443"}"#,
        r#"{"version":1,"listen":"127.0.0.1:0","advertised":"[::1]:443"}"#,
        r#"{"version":1,"listen":"127.0.0.1:0","advertised":"127.0.0.1:0","private_key":"forbidden"}"#,
        r#"{"version":1,"listen":"127.0.0.1:0","advertised":"127.0.0.1:0","max_connections":0}"#,
        r#"{"version":1,"listen":"127.0.0.1:0","advertised":"127.0.0.1:0","max_connections":65}"#,
    ] {
        std::fs::write(&path, invalid)?;
        configure(&fixture, "--set-self-owned-tls", path.as_os_str(), false).await?;
        ensure!(std::fs::read(&policy_path)? == before);
    }
    // Valid settings but occupied port: startup is terminal, not stale serving.
    let occupied = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = occupied.local_addr()?.to_string();
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version":1,"listen":address,"advertised":address,
        }))?,
    )?;
    configure(&fixture, "--set-self-owned-tls", path.as_os_str(), true).await?;
    let mut failed = fixture.launch_owned("--foreground")?;
    failed.finish(false).await?;
    let status: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.0.join("status.json"))?)?;
    ensure!(status["phase"] == "failed_closed");
    Ok(())
}
