//! Headless signed NAT observer and punch runtime.

use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    attempt_signed_nat_punch_v1, request_observed_endpoint_v1, serve_nat_punch_once_v1,
    serve_observed_endpoint_once_v1, NatPunchAttemptV1,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::{SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductNatRuntimeModeV1 {
    ObservedEndpointObserver,
    NatPunchTarget,
    ObservedEndpointProbe,
    NatPunchProbe,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProductNatRuntimeConfigV1 {
    pub mode: ProductNatRuntimeModeV1,
    pub bind_addr: String,
    pub identity_key_path: PathBuf,
    #[serde(default)]
    pub peer_addr: Option<String>,
    #[serde(default)]
    pub expected_peer_id: Option<String>,
    #[serde(default = "default_timeout_ms_v1")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub relay_candidate_available: bool,
    #[serde(default)]
    pub report_path: Option<PathBuf>,
    #[serde(default)]
    pub run_for_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductNatRuntimeReportV1 {
    pub accepted: bool,
    pub scope: &'static str,
    pub mode: String,
    pub bind_addr: String,
    pub observed_endpoint: Option<String>,
    pub punch_attempt: Option<NatPunchAttemptV1>,
    pub network_only: bool,
    pub payload_treated_opaque: bool,
    pub apfl_interpreted: bool,
    pub aoem_called: bool,
    pub ledger_semantics: bool,
    pub novorudp_wire_changed: bool,
}

pub fn load_product_nat_runtime_config_v1(
    path: impl AsRef<Path>,
) -> Result<ProductNatRuntimeConfigV1> {
    let path = path.as_ref();
    let bytes =
        fs::read(path).with_context(|| format!("read NAT runtime config: {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("decode NAT runtime config: {}", path.display()))
}

pub fn run_product_nat_runtime_v1(
    config: ProductNatRuntimeConfigV1,
) -> Result<ProductNatRuntimeReportV1> {
    if config.timeout_ms == 0 {
        bail!("timeout_ms must be positive");
    }
    let identity = load_ed25519_key_v1(&config.identity_key_path)?;
    let socket = UdpSocket::bind(&config.bind_addr)
        .with_context(|| format!("bind NAT runtime: {}", config.bind_addr))?;
    let bind_addr = socket
        .local_addr()
        .context("read NAT runtime address")?
        .to_string();
    let timeout = Duration::from_millis(config.timeout_ms);
    let report = match config.mode {
        ProductNatRuntimeModeV1::ObservedEndpointObserver => {
            serve_loop_v1(&socket, &identity, &config, |socket, identity| {
                serve_observed_endpoint_once_v1(
                    socket,
                    identity,
                    config.timeout_ms.saturating_add(5_000),
                )
            })?;
            base_report_v1("observed_endpoint_observer", bind_addr, None, None)
        }
        ProductNatRuntimeModeV1::NatPunchTarget => {
            serve_loop_v1(&socket, &identity, &config, |socket, identity| {
                serve_nat_punch_once_v1(socket, identity, config.timeout_ms.saturating_add(5_000))
            })?;
            base_report_v1("nat_punch_target", bind_addr, None, None)
        }
        ProductNatRuntimeModeV1::ObservedEndpointProbe => {
            let peer_addr = parse_required_peer_addr_v1(&config)?;
            let expected_peer_id = required_expected_peer_id_v1(&config)?;
            let ack = request_observed_endpoint_v1(
                &socket,
                peer_addr,
                &identity,
                expected_peer_id,
                timeout,
            )
            .context("run signed observed endpoint probe")?;
            base_report_v1(
                "observed_endpoint_probe",
                bind_addr,
                Some(ack.observed_endpoint),
                None,
            )
        }
        ProductNatRuntimeModeV1::NatPunchProbe => {
            let peer_addr = parse_required_peer_addr_v1(&config)?;
            let expected_peer_id = required_expected_peer_id_v1(&config)?;
            let attempt = attempt_signed_nat_punch_v1(
                &socket,
                peer_addr,
                &identity,
                expected_peer_id,
                timeout,
                config.relay_candidate_available,
            );
            base_report_v1("nat_punch_probe", bind_addr, None, Some(attempt))
        }
    };
    if let Some(path) = config.report_path.as_deref() {
        write_report_v1(path, &report)?;
    }
    Ok(report)
}

fn serve_loop_v1<F>(
    socket: &UdpSocket,
    identity: &SigningKey,
    config: &ProductNatRuntimeConfigV1,
    mut serve_once: F,
) -> Result<()>
where
    F: FnMut(&UdpSocket, &SigningKey) -> Result<(), novovm_network::ProductNatErrorV1>,
{
    let started_at_ms = now_ms_v1();
    socket
        .set_read_timeout(Some(Duration::from_millis(config.timeout_ms.min(1_000))))
        .context("set NAT server read timeout")?;
    loop {
        if config
            .run_for_ms
            .is_some_and(|duration| now_ms_v1().saturating_sub(started_at_ms) >= duration)
        {
            return Ok(());
        }
        match serve_once(socket, identity) {
            Ok(()) => {}
            Err(novovm_network::ProductNatErrorV1::Io(_)) => continue,
            Err(error) => eprintln!("NAT runtime rejected datagram: {error}"),
        }
    }
}

fn base_report_v1(
    mode: &str,
    bind_addr: String,
    observed_endpoint: Option<String>,
    punch_attempt: Option<NatPunchAttemptV1>,
) -> ProductNatRuntimeReportV1 {
    ProductNatRuntimeReportV1 {
        accepted: true,
        scope: "novovm_product_nat_runtime_v1",
        mode: mode.into(),
        bind_addr,
        observed_endpoint,
        punch_attempt,
        network_only: true,
        payload_treated_opaque: true,
        apfl_interpreted: false,
        aoem_called: false,
        ledger_semantics: false,
        novorudp_wire_changed: false,
    }
}

fn parse_required_peer_addr_v1(config: &ProductNatRuntimeConfigV1) -> Result<SocketAddr> {
    config
        .peer_addr
        .as_deref()
        .context("peer_addr is required for NAT probe mode")?
        .parse()
        .context("parse NAT peer_addr")
}

fn required_expected_peer_id_v1(config: &ProductNatRuntimeConfigV1) -> Result<&str> {
    config
        .expected_peer_id
        .as_deref()
        .context("expected_peer_id is required for NAT probe mode")
}

fn load_ed25519_key_v1(path: &Path) -> Result<SigningKey> {
    let key = fs::read_to_string(path)
        .with_context(|| format!("read NAT identity key: {}", path.display()))?;
    let key = key.trim();
    if key.len() != 64 {
        bail!("NAT identity key must contain exactly 64 hexadecimal characters");
    }
    let mut bytes = [0u8; 32];
    for (index, output) in bytes.iter_mut().enumerate() {
        *output = u8::from_str_radix(&key[index * 2..index * 2 + 2], 16)
            .context("decode NAT identity key hex")?;
    }
    Ok(SigningKey::from_bytes(&bytes))
}

fn write_report_v1(path: &Path, report: &ProductNatRuntimeReportV1) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create NAT report directory: {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_vec_pretty(report)?)
        .with_context(|| format!("write NAT report: {}", path.display()))
}

fn default_timeout_ms_v1() -> u64 {
    3_000
}
fn now_ms_v1() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_modes_require_a_peer_and_expected_identity() {
        let config = ProductNatRuntimeConfigV1 {
            mode: ProductNatRuntimeModeV1::NatPunchProbe,
            bind_addr: "127.0.0.1:0".into(),
            identity_key_path: "identity.hex".into(),
            peer_addr: None,
            expected_peer_id: None,
            timeout_ms: 1,
            relay_candidate_available: false,
            report_path: None,
            run_for_ms: None,
        };
        assert!(parse_required_peer_addr_v1(&config).is_err());
        assert!(required_expected_peer_id_v1(&config).is_err());
    }
}
