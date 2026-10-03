//! Headless signed NAT observer and punch runtime.

use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    attempt_signed_nat_punch_v1, request_observed_endpoint_v1, serve_nat_punch_once_v1,
    serve_observed_endpoint_once_v1, NatPunchAttemptV1, NatSelectedPathV1,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::{SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductNatRuntimeModeV1 {
    ObservedEndpointObserver,
    NatPunchTarget,
    ObservedEndpointProbe,
    NatPunchProbe,
    NatPunchMonitor,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub quality_samples: Vec<ProductNatQualitySampleV1>,
    pub network_only: bool,
    pub payload_treated_opaque: bool,
    pub apfl_interpreted: bool,
    pub aoem_called: bool,
    pub ledger_semantics: bool,
    pub novorudp_wire_changed: bool,
}

/// Local diagnostic samples, not a command to migrate application traffic.
#[derive(Debug, Clone, Serialize)]
pub struct ProductNatQualitySampleV1 {
    pub elapsed_ms: u64,
    pub probe_response_ms: Option<u64>,
    pub smoothed_response_ms: Option<u64>,
    pub consecutive_failures: u32,
    pub suggested_path: NatSelectedPathV1,
}

#[derive(Default)]
struct DirectProbeHealthV1 {
    direct: bool,
    ever_direct: bool,
    successes: u32,
    failures: u32,
    smoothed_ms: Option<u64>,
}
impl DirectProbeHealthV1 {
    fn observe(
        &mut self,
        valid: bool,
        elapsed: Duration,
        relay_available: bool,
    ) -> NatSelectedPathV1 {
        if valid {
            self.failures = 0;
            self.successes = self.successes.saturating_add(1);
            let sample = elapsed.as_millis().min(u64::MAX as u128) as u64;
            self.smoothed_ms = Some(match self.smoothed_ms {
                Some(previous) => ((u128::from(previous) * 7 + u128::from(sample)) / 8) as u64,
                None => sample,
            });
            if !self.ever_direct || self.successes >= 2 {
                self.direct = true;
                self.ever_direct = true;
            }
        } else {
            self.successes = 0;
            self.failures = self.failures.saturating_add(1);
            if self.failures >= 3 {
                self.direct = false;
                self.smoothed_ms = None;
            }
        }
        if self.direct {
            NatSelectedPathV1::PunchedDirect
        } else if relay_available {
            NatSelectedPathV1::RelayNovoRudp
        } else {
            NatSelectedPathV1::QueueFallback
        }
    }
}

fn monitor_direct_v1(
    socket: &UdpSocket,
    identity: &SigningKey,
    config: &ProductNatRuntimeConfigV1,
) -> Result<Vec<ProductNatQualitySampleV1>> {
    let run_ms = config
        .run_for_ms
        .context("nat_punch_monitor requires run_for_ms")?;
    if !(1..=120_000).contains(&run_ms) {
        bail!("nat_punch_monitor run_for_ms must be 1..120000");
    }
    let address = parse_required_peer_addr_v1(config)?;
    let peer = required_expected_peer_id_v1(config)?;
    let start = Instant::now();
    let budget = Duration::from_millis(run_ms);
    let mut health = DirectProbeHealthV1::default();
    let mut samples = Vec::new();
    while start.elapsed() < budget && samples.len() < 120 {
        let probe_started = Instant::now();
        let timeout =
            Duration::from_millis(config.timeout_ms).min(budget.saturating_sub(start.elapsed()));
        if timeout.is_zero() {
            break;
        }
        let attempt = attempt_signed_nat_punch_v1(
            socket,
            address,
            identity,
            peer,
            timeout,
            config.relay_candidate_available,
        );
        let elapsed = probe_started.elapsed();
        let path = health.observe(attempt.ack_valid, elapsed, config.relay_candidate_available);
        samples.push(ProductNatQualitySampleV1 {
            elapsed_ms: start.elapsed().as_millis() as u64,
            probe_response_ms: attempt.ack_valid.then_some(elapsed.as_millis() as u64),
            smoothed_response_ms: health.smoothed_ms,
            consecutive_failures: health.failures,
            suggested_path: path,
        });
        // At most one probe per second, no catch-up burst after a slow probe.
        let pause = Duration::from_secs(1)
            .saturating_sub(probe_started.elapsed())
            .min(budget.saturating_sub(start.elapsed()));
        if !pause.is_zero() {
            std::thread::sleep(pause);
        }
    }
    Ok(samples)
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
        ProductNatRuntimeModeV1::NatPunchMonitor => {
            let samples = monitor_direct_v1(&socket, &identity, &config)?;
            let mut report = base_report_v1("nat_punch_monitor", bind_addr, None, None);
            report.quality_samples = samples;
            report
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
        quality_samples: Vec::new(),
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
    fn direct_probe_hysteresis_requires_failures_and_recovery_streaks() {
        let mut health = DirectProbeHealthV1::default();
        let dt = Duration::from_millis(50);
        assert_eq!(
            health.observe(true, dt, true),
            NatSelectedPathV1::PunchedDirect
        );
        for _ in 0..2 {
            assert_eq!(
                health.observe(false, dt, true),
                NatSelectedPathV1::PunchedDirect
            );
        }
        assert_eq!(
            health.observe(false, dt, true),
            NatSelectedPathV1::RelayNovoRudp
        );
        assert_eq!(health.smoothed_ms, None);
        assert_eq!(
            health.observe(true, dt, true),
            NatSelectedPathV1::RelayNovoRudp
        );
        assert_eq!(
            health.observe(false, dt, true),
            NatSelectedPathV1::RelayNovoRudp
        );
        assert_eq!(
            health.observe(true, dt, true),
            NatSelectedPathV1::RelayNovoRudp
        );
        assert_eq!(
            health.observe(true, dt, true),
            NatSelectedPathV1::PunchedDirect
        );
        let mut unknown = DirectProbeHealthV1::default();
        assert_eq!(
            unknown.observe(false, dt, false),
            NatSelectedPathV1::QueueFallback
        );
    }

    #[test]
    fn direct_monitor_collects_authenticated_udp_samples() {
        let target_key = SigningKey::from_bytes(&[217; 32]);
        let peer = novovm_network::peer_id_from_ed25519_public_key_v1(
            &target_key.verifying_key().to_bytes(),
        );
        let target = UdpSocket::bind("127.0.0.1:0").unwrap();
        target
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let address = target.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                serve_nat_punch_once_v1(&target, &target_key, 5000).unwrap();
            }
        });
        let config = ProductNatRuntimeConfigV1 {
            mode: ProductNatRuntimeModeV1::NatPunchMonitor,
            bind_addr: "127.0.0.1:0".into(),
            identity_key_path: "unused".into(),
            peer_addr: Some(address.to_string()),
            expected_peer_id: Some(peer),
            timeout_ms: 500,
            relay_candidate_available: true,
            report_path: None,
            run_for_ms: Some(1500),
        };
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let samples =
            monitor_direct_v1(&socket, &SigningKey::from_bytes(&[218; 32]), &config).unwrap();
        server.join().unwrap();
        assert_eq!(samples.len(), 2);
        assert!(samples.iter().all(|s| s.probe_response_ms.is_some()
            && s.suggested_path == NatSelectedPathV1::PunchedDirect));
        assert_eq!(socket.read_timeout().unwrap(), None);
        let invalid = ProductNatRuntimeConfigV1 {
            run_for_ms: None,
            ..config
        };
        assert!(monitor_direct_v1(&socket, &SigningKey::from_bytes(&[218; 32]), &invalid).is_err());
    }

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
