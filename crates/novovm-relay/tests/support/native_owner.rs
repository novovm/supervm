//! Real executable CLI migration with the original Windows custody and journal.
use super::*;
use novovm_network::product_service_lease::NativeMailboxEndpointV7;

async fn configure(f: &Fixture, value: &std::ffi::OsStr, success: bool) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let mut child = Process(
        Command::new(env!("CARGO_BIN_EXE_novovm-rendezvous-service"))
            .arg("--set-self-owned-native")
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

#[tokio::test(flavor = "current_thread")]
async fn windows_native_cli_preserves_custody_across_activation_restart_and_off() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let fixture = Fixture::new()?;
        fixture.owned_config()?;
        let initial = fixture.launch_owned("--provision-self-owned")?;
        let first = fixture.owned_record(0).await?;
        let original_iroh = IrohServiceEndpointV1::from_record(&first)?;
        let settings = fixture.0.join("native.json");
        std::fs::write(
            &settings,
            br#"{"version":1,"listen":"127.0.0.1:0","advertised":"127.0.0.1:0","max_connections":8,"max_bytes_per_minute":1048576}"#,
        )?;
        let policy_path = fixture.0.join("self-owned-policy.dpapi");
        let original_policy = std::fs::read(&policy_path)?;
        configure(&fixture, settings.as_os_str(), false).await?;
        ensure!(std::fs::read(&policy_path)? == original_policy);
        drop(initial);

        let preserved = ["owned.json", "keys.dpapi", "service.journal", "mailbox.sqlite"];
        let before = preserved
            .iter()
            .map(|name| std::fs::read(fixture.0.join(name)))
            .collect::<std::io::Result<Vec<_>>>()?;
        configure(&fixture, settings.as_os_str(), true).await?;
        for (name, bytes) in preserved.iter().zip(before) {
            ensure!(std::fs::read(fixture.0.join(name))? == bytes, "{name} altered by selection");
        }
        let native = fixture.launch_owned("--foreground")?;
        let second = fixture.owned_record(first.sequence).await?;
        validate_relay_record_v1(&second, now_ms()?)?;
        ensure!(second.endpoints.len() == 1, "mixed carrier publication");
        let route = NativeMailboxEndpointV7::from_record(&second)?.context("native route missing")?;
        ensure!(route.address.ip().is_loopback() && route.address.port() != 0);
        ensure!(route.max_connections == 8 && route.max_bytes_per_minute == 1048576);
        ensure!(second.relay_public_key == first.relay_public_key);
        ensure!(IrohServiceEndpointV1::from_record(&second).is_err(), "stale Iroh route published");
        let keys = std::fs::read(fixture.0.join("keys.dpapi"))?;
        drop(native);

        let restarted = fixture.launch_owned("--foreground")?;
        let third = fixture.owned_record(second.sequence).await?;
        validate_relay_record_v1(&third, now_ms()?)?;
        ensure!(third.relay_public_key == first.relay_public_key);
        ensure!(NativeMailboxEndpointV7::from_record(&third)?.is_some());
        ensure!(std::fs::read(fixture.0.join("keys.dpapi"))? == keys);
        drop(restarted);

        configure(&fixture, "off".as_ref(), true).await?;
        let _restored = fixture.launch_owned("--foreground")?;
        let fourth = fixture.owned_record(third.sequence).await?;
        ensure!(fourth.relay_public_key == first.relay_public_key);
        ensure!(NativeMailboxEndpointV7::from_record(&fourth)?.is_none());
        ensure!(IrohServiceEndpointV1::from_record(&fourth)?.address.endpoint_key == original_iroh.address.endpoint_key);
        ensure!(std::fs::read(fixture.0.join("keys.dpapi"))? == keys);
        Ok(())
    }).await?
}
