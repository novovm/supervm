use super::*;
use novovm_network::product_overlay::TransportVerifier;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

/// Only the cached-header verifier pays this delay. All signatures still go
/// through the original AOEM; the authenticated archive and receipt use the
/// normal production network deadline and original verification path.
struct SlowVerifier {
    identity: TransportIdentity,
    first: AtomicBool,
    observed: Arc<Mutex<Option<(Instant, Instant)>>>,
}

impl TransportVerifier for SlowVerifier {
    fn verify(&self, key: &[u8], message: &[u8], context: &[u8], signature: &[u8]) -> Result<()> {
        let start = Instant::now();
        if !self.first.swap(true, Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(11));
        }
        self.identity.verify(key, message, context, signature)?;
        let mut times = self.observed.lock().unwrap();
        let times = times.get_or_insert((start, start));
        times.1 = Instant::now();
        Ok(())
    }
}

pub(in crate::native_pipeline::service) fn test_slow_cached_receipt(
    path: &Path,
    pin: &str,
    device: &Path,
    hash: &str,
    height: u64,
) -> Result<serde_json::Value> {
    let transaction = transaction_hash(hash)?;
    let (profile, domain, anchor) = installed_chain_profile(path, Some(pin))?;
    let base = profile_base(path)?;
    let identity = client_identity(device, base, &profile, false)?;
    let search = chain_sync::ReceiptSearch::open(device, &profile, anchor, transaction)?;
    let observed = Arc::new(Mutex::new(None));
    let verifier = Arc::new(FinalityVerifier::new(
        &domain,
        SlowVerifier {
            identity: identity.clone(),
            first: AtomicBool::new(false),
            observed: observed.clone(),
        },
    ));
    let started = Instant::now();
    let mut report = search.execute(
        base,
        &profile,
        &identity,
        &domain,
        ReceiptLocation::QueryByHash,
        verifier,
    )?;
    let (first, last) = observed
        .lock()
        .unwrap()
        .context("expected an authentic cached header")?;
    let local_ms = (last - first).as_millis() as u64;
    ensure!(
        local_ms >= 11_000 && report["height"] == height,
        "slow local verification regression"
    );
    report["local_verification_elapsed_ms"] = local_ms.into();
    report["injected_local_delay_ms"] = 11_000.into();
    report["end_to_end_elapsed_ms"] = (started.elapsed().as_millis() as u64).into();
    Ok(report)
}
