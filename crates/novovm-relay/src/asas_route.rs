//! A's ASAS application consumes the existing NOVONET-authenticated principal.
//! This module owns only bounded IPC dispatch, never a socket or node key.
use anyhow::{ensure, Context, Result};
use novovm_asas_plugin::{Worker, MAX_FRAME_BYTES};
use novovm_network::native_application::{
    ApplicationFutureV1, AuthenticatedApplicationPeerV1, NativeApplicationHandlerV1,
    NativeApplicationRouteV1, MAX_JOBS,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
pub(crate) mod auth;

pub const STREAM: u64 = u64::from_be_bytes(*b"NVASAS01");
pub fn domain() -> [u8; 32] {
    Sha256::digest(b"novonet.application.asas.v1\0").into()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    operation: String,
    #[serde(default)]
    input: Value,
}
struct Work {
    peer: AuthenticatedApplicationPeerV1,
    principal: String,
    request: Request,
    reply: oneshot::Sender<Result<Vec<u8>>>,
}
struct Handler {
    peers: BTreeMap<String, String>,
    queue: mpsc::Sender<Work>,
    available: Arc<AtomicBool>,
}
impl NativeApplicationHandlerV1 for Handler {
    fn closed(&self) {
        self.available.store(false, Ordering::Release);
    }
    fn request(&self, peer: AuthenticatedApplicationPeerV1, input: Vec<u8>) -> ApplicationFutureV1 {
        let principal = self.peers.get(peer.id()).cloned();
        let queue = self.queue.clone();
        let available = self.available.clone();
        Box::pin(async move {
            ensure!(
                available.load(Ordering::Acquire) && peer.is_current(),
                "ASAS unavailable or session retired"
            );
            let principal = principal.context("ASAS node has no application grant")?;
            ensure!(
                input.len() < MAX_FRAME_BYTES,
                "ASAS application request budget"
            );
            let mut request: Request = serde_json::from_slice(&input)?;
            ensure!(
                [
                    "capabilities",
                    "submit",
                    "events",
                    "result",
                    "cancel",
                    "login_begin",
                    "login_complete"
                ]
                .contains(&request.operation.as_str()),
                "ASAS unsupported network operation"
            );
            if request.operation == "events" {
                let object = request
                    .input
                    .as_object_mut()
                    .context("ASAS event input required")?;
                object.insert("waitMs".into(), 1000.into());
            }
            let (reply, result) = oneshot::channel();
            queue
                .try_send(Work {
                    peer,
                    principal,
                    request,
                    reply,
                })
                .map_err(|_| anyhow::anyhow!("ASAS busy or unavailable"))?;
            result
                .await
                .context("ASAS worker disconnected; inspect original task")?
        })
    }
}
pub struct RouteDriverV1 {
    worker: Worker,
    queue: mpsc::Receiver<Work>,
    available: Arc<AtomicBool>,
    login: Option<auth::LoginGate>,
}
impl RouteDriverV1 {
    pub fn new(
        worker: Worker,
        peers: BTreeMap<String, String>,
    ) -> (Self, NativeApplicationRouteV1) {
        let (queue, receive) = mpsc::channel(MAX_JOBS);
        let available = Arc::new(AtomicBool::new(true));
        let handler = Arc::new(Handler {
            peers,
            queue,
            available: available.clone(),
        });
        (
            Self {
                worker,
                queue: receive,
                available,
                login: None,
            },
            NativeApplicationRouteV1 {
                stream: STREAM,
                domain: domain(),
                handler,
            },
        )
    }
    pub async fn with_uca(
        worker: Worker,
        peers: BTreeMap<String, String>,
        config: &novovm_asas_plugin::UcaAuthorityConfigV1,
        identity: novovm_network::product_overlay::TransportIdentity,
    ) -> Result<(Self, NativeApplicationRouteV1)> {
        let gate = auth::LoginGate::start(config, identity).await?;
        let (mut driver, route) = Self::new(worker, peers);
        driver.login = Some(gate);
        Ok((driver, route))
    }
    pub async fn run(&mut self) -> Result<()> {
        let result = self.run_inner().await;
        self.available.store(false, Ordering::Release);
        self.queue.close();
        result
    }
    async fn run_inner(&mut self) -> Result<()> {
        loop {
            let work = tokio::select! {
                _ = self.worker.unexpected_exit() => anyhow::bail!("ASAS worker exited"),
                work = self.queue.recv() => match work {
                    Some(work) => work,
                    None => anyhow::bail!("ASAS route closed; other L3 services continue"),
                },
            };
            if work.reply.is_closed() || !work.peer.is_current() {
                continue;
            }
            let operation = work.request.operation;
            let result = async {
                if let Some(gate) = &mut self.login {
                    if let Some(response) = gate
                        .request(&work.peer, &operation, &work.request.input)
                        .await?
                    {
                        ensure!(work.peer.is_current(), "login session retired");
                        return Ok(serde_json::to_vec(&response)?);
                    }
                } else {
                    ensure!(
                        !operation.starts_with("login_"),
                        "UCA login not configured on A"
                    );
                }
                self.worker
                    .request_from_node(
                        &work.principal,
                        Some(work.peer.id()),
                        &operation,
                        work.request.input,
                    )
                    .await
                    .and_then(|mut value| {
                        ensure!(
                            work.peer.is_current(),
                            "ASAS session retired while executing; no replay"
                        );
                        if operation == "capabilities" {
                            value["remoteRouteReady"] = true.into();
                            value["ucaLogin"] = self
                                .login
                                .as_ref()
                                .map(|gate| gate.status(&work.peer))
                                .unwrap_or_else(
                                    || serde_json::json!({"version":1,"status":"not_configured"}),
                                );
                        }
                        let bytes = serde_json::to_vec(&value)?;
                        ensure!(bytes.len() < MAX_FRAME_BYTES, "ASAS result budget");
                        Ok(bytes)
                    })
            }
            .await;
            let _ = work.reply.send(result);
        }
    }
    pub async fn shutdown(&mut self) -> Result<()> {
        self.available.store(false, Ordering::Release);
        self.queue.close();
        tokio::time::timeout(Duration::from_secs(31), self.worker.shutdown()).await?
    }
}

#[cfg(test)]
#[path = "asas_route_tests.rs"]
mod tests;
