use super::*;
use crate::transport_identity::component_fixture as identity;
use novovm_network::{
    native_application::{
        lane, NativeApplicationClientV1, NativeApplicationDispatcherV1, DEADLINE, MAX_WIRE_BYTES,
    },
    novorudp::{
        endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
        object_link::{NativeObjectLink, NativeSendPolicy},
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    },
    product_overlay::peer_id_from_mldsa65_public_key_v1,
};
use std::{
    net::UdpSocket,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn network_envelope_cannot_supply_a_principal_or_unknown_operation() {
    assert!(serde_json::from_value::<Request>(
        serde_json::json!({"operation":"submit","peerId":"privileged","input":{}})
    )
    .is_err());
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "explicit local built Node worker required; no inference, no production node"]
async fn native_udp_to_real_node_is_principal_bound_and_does_not_require_acode_on_a() -> Result<()>
{
    let node =
        PathBuf::from(std::env::var_os("ASAS_TEST_NODE").context("ASAS_TEST_NODE required")?);
    let entry =
        PathBuf::from(std::env::var_os("ASAS_TEST_ENTRY").context("ASAS_TEST_ENTRY required")?);
    let root = std::env::temp_dir().join(format!(
        "asas-route-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    std::fs::create_dir(&root)?;
    let _cleanup = Cleanup(root.clone());
    let config_path = root.join("service.json");
    std::fs::write(
        &config_path,
        serde_json::to_vec(
            &serde_json::json!({"directory":root.join("data"),"models":["fixture-no-inference"],
        "peers":[{"id":"B","tokenSha256":"a".repeat(64),"projects":["b".repeat(64)]}]}),
        )?,
    )?;
    let config = novovm_asas_plugin::Config {
        version: 1,
        node,
        entry_sha256: format!("{:x}", Sha256::digest(std::fs::read(&entry)?)),
        entry,
        service_config: config_path,
        shutdown_ms: 5000,
        network_peers: Default::default(),
    };
    let worker = Worker::start(&config).await?;
    let keys = [
        identity::component_identity([91; 32]),
        identity::component_identity([92; 32]),
    ];
    let allowed = peer_id_from_mldsa65_public_key_v1(keys[1].public_key());
    let (mut driver, route) = RouteDriverV1::new(worker, [(allowed, "B".into())].into());
    let sockets = [
        UdpSocket::bind("127.0.0.1:0")?,
        UdpSocket::bind("127.0.0.1:0")?,
    ];
    let addresses = [sockets[0].local_addr()?, sockets[1].local_addr()?];
    let mut links = Vec::new();
    for (index, socket) in sockets.into_iter().enumerate() {
        let endpoint = NativeEndpoint::bind(
            socket,
            keys[index].clone(),
            vec![NativePeer {
                key: keys[1 - index].public_key().to_vec(),
                address: addresses[1 - index],
            }],
            NativeHandshakeBudget {
                lifetime: DEADLINE,
                retry: Duration::from_millis(10),
            },
            1000,
            Instant::now(),
        )?;
        links.push(NativeObjectLink::new(
            endpoint,
            vec![lane(STREAM, domain(), MAX_WIRE_BYTES * 2)?],
            NativeSendPolicy {
                window: NovoRudpWindowConfig::default(),
                pacing: NovoRudpPacingProfile::default(),
                ttl: DEADLINE,
            },
        )?);
    }
    let mut b = links.pop().unwrap();
    let mut a = links.pop().unwrap();
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let exchange = async {
            while a.session_id(0).is_none() || b.session_id(0).is_none() {
                let now=Instant::now(); let _=a.poll(1000,now,64)?;let _=b.poll(1000,now,64)?;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let mut dispatch = NativeApplicationDispatcherV1::new(route);
            let mut client = NativeApplicationClientV1::attach(&mut b,0,STREAM,domain())?;
            client.begin(br#"{"operation":"capabilities"}"#,Instant::now())?;
            let mut response = None;
            while response.is_none() {
                let now=Instant::now();let _=a.poll(1000,now,64)?;let _=b.poll(1000,now,64)?;
                dispatch.poll(&mut a,now,None)?;response=client.poll(&mut b,now)?;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let capabilities:Value=serde_json::from_slice(&response.unwrap())?;
            assert_eq!(capabilities["peerId"],"B");assert_eq!(capabilities["projects"][0],"b".repeat(64));
            assert_eq!(capabilities["remoteRouteReady"],true);
            client.begin(br#"{"operation":"capabilities","peerId":"privileged"}"#,Instant::now())?;
            loop {
                let now=Instant::now();let _=a.poll(1000,now,64)?;let _=b.poll(1000,now,64)?;
                dispatch.poll(&mut a,now,None)?;
                if client.poll(&mut b,now).is_err(){break;}
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            Ok::<_,anyhow::Error>(())
        };
        tokio::select! { result=exchange=>result, result=driver.run()=>{result?;anyhow::bail!("worker unexpectedly stopped")} }
    }).await;
    let drain = driver.shutdown().await;
    result??;
    drain?;
    ensure!(
        !root.join("data/owner.lock").exists(),
        "Node worker did not drain"
    );
    Ok(())
}
