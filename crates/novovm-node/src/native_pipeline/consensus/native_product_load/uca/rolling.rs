//! Legal rolling NUAS workload, not a production protocol or authorization
//! change. At most 256 unique transactions are supplied between common-head
//! fences; the original nonempty-candidate rule bounds their height advance.
//! This is a different load strategy from the retained 8192-window static A/B.
use super::*;
use novovm_protocol::uca_spend::MAX_SPEND_WINDOW;

pub(super) const WAVE_ROWS: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LoadMode {
    Static,
    Rolling,
}

impl LoadMode {
    pub(super) fn is_rolling(self) -> bool {
        self == Self::Rolling
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Static => "static_signed_corpus_8192_outstanding_limit",
            Self::Rolling => "rolling_256_transaction_common_head_waves",
        }
    }

    pub(super) fn validate_count(self, count: usize, frame_rows: usize) -> Result<()> {
        if self.is_rolling() {
            ensure!(
                count >= WAVE_ROWS
                    && count.is_multiple_of(WAVE_ROWS)
                    && WAVE_ROWS.is_multiple_of(frame_rows),
                "rolling load requires complete 256-row waves and whole wire frames"
            );
        }
        Ok(())
    }

    pub(super) fn grant_expiry(self, count: usize) -> Result<u64> {
        if self.is_rolling() {
            u64::try_from(count)?
                .checked_add(MAX_SPEND_WINDOW + 1)
                .context("rolling fixture grant height exhausted")
        } else {
            Ok(1000)
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SpendWindow {
    pub not_before: u64,
    pub expires: u64,
}

impl SpendWindow {
    pub(super) fn after(height: u64) -> Result<Self> {
        let not_before = height.checked_add(1).context("rolling height exhausted")?;
        let expires = not_before
            .checked_add(MAX_SPEND_WINDOW)
            .context("rolling spend window exhausted")?;
        Ok(Self {
            not_before,
            expires,
        })
    }

    fn verify_completed_wave(self, previous: u64, current: u64, rows: usize) -> Result<()> {
        ensure!(
            previous.checked_add(1) == Some(self.not_before)
                && current >= self.not_before
                && current < self.expires
                && current - previous <= rows as u64,
            "rolling wave exceeded its unchanged legal spend window/nonempty-height bound"
        );
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_signed_rows(
    raw: &mut Vec<Vec<u8>>,
    observations: &mut Vec<Observation>,
    workload_hash: &mut Sha256,
    count: usize,
    window: SpendWindow,
    fee: u128,
    make_signed: &impl Fn(usize, SpendWindow) -> Result<SignedAssetSpendV2>,
) -> Result<()> {
    ensure!(
        raw.len() == observations.len(),
        "rolling raw/observation index mismatch"
    );
    for _ in 0..count {
        let index = raw.len();
        let signed = make_signed(index, window)?;
        ensure!(
            signed.intent.payer == account_bytes(index % SENDERS)
                && signed.intent.recipient == account_bytes(SENDERS)
                && signed.intent.nonce == (index / SENDERS) as u64
                && signed.intent.amount == 1
                && signed.intent.max_fee == 50
                && signed.intent.not_before_height == window.not_before
                && signed.intent.expires_at_height == window.expires,
            "rolling signer changed logical workload/window"
        );
        let hash = Sha256::digest(signed.intent.signing_bytes()?).into();
        let bytes = signed.encode()?;
        workload_hash.update((bytes.len() as u64).to_be_bytes());
        workload_hash.update(&bytes);
        observations.push(Observation {
            hash,
            first_submit_us: None,
            receipt_query_observed_us: [None; 4],
            receipt: None,
            expected_nonce: (index / SENDERS) as u64 + 1,
            expected_fee: fee,
        });
        raw.push(bytes);
    }
    Ok(())
}

pub(super) fn prepare_inputs(
    client: &mut NativeApplicationClient,
    raw: &[Vec<u8>],
    _first_index: usize,
    frame_rows: usize,
) -> Result<Vec<PreparedApplicationInput>> {
    raw.chunks(frame_rows)
        .map(|rows| {
            client.prepare_asset_spend(Arc::new(AssetSpendBatchV2::from_raw(
                rows,
                AssetSpendBatchLimitsV2 {
                    transactions: frame_rows,
                    transaction_bytes: MAX_SIGNED_SPEND_BYTES,
                    body_bytes: CANONICAL_BODY_BYTES,
                },
            )?))
        })
        .collect()
}

fn common_head(
    nodes: &mut ProductNodes,
    clients: &mut [NativeApplicationClient],
    deadline: Instant,
) -> Result<Status> {
    loop {
        ensure!(
            Instant::now() < deadline,
            "rolling common-head overall deadline"
        );
        nodes.alive()?;
        let statuses = clients
            .iter_mut()
            .map(|client| {
                let Answer::Status(status) = query_before(client, Query::Status, deadline)? else {
                    bail!("rolling native status missing");
                };
                ensure!(!status.projection_failed, "rolling projection failure");
                Ok(status)
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            statuses.len() == 4,
            "rolling common-head requires four original nodes"
        );
        if statuses.iter().all(|status| !status.recovering)
            && same_head(&statuses, statuses[0]).is_ok()
        {
            return Ok(statuses[0]);
        }
        std::thread::yield_now();
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn drive_rolling(
    nodes: &mut ProductNodes,
    clients: &mut [NativeApplicationClient],
    ingress: usize,
    mut inputs: Vec<PreparedApplicationInput>,
    frame_rows: usize,
    raw: &mut Vec<Vec<u8>>,
    observations: &mut Vec<Observation>,
    workload_hash: &mut Sha256,
    counts: &mut Counts,
    timeout: Duration,
    count: usize,
    fee: u128,
    make_signed: &impl Fn(usize, SpendWindow) -> Result<SignedAssetSpendV2>,
    evidence: &mut Value,
) -> Result<Duration> {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut offset = 0;
    let mut parent_height = 0;
    let mut window = SpendWindow::after(0)?;
    let mut waves = Vec::new();
    let mut timed_signing_us = 0u64;
    let mut timed_preparation_us = 0u64;
    let mut timed_head_queries_us = 0u64;
    let result = (|| -> Result<Duration> {
        loop {
            ensure!(
                raw.len() == offset + WAVE_ROWS,
                "rolling wave construction drift"
            );
            let last_wave = offset + WAVE_ROWS == count;
            drive_at(
                nodes,
                clients,
                ingress,
                &inputs,
                frame_rows,
                &mut observations[offset..],
                counts,
                timeout,
                start,
                !last_wave,
            )?;
            ensure!(
                counts.maximum_outstanding <= WAVE_ROWS,
                "rolling outstanding budget exceeded"
            );
            // Only BETWEEN-wave fences belong inside the total wall. The last
            // wave keeps the original last-four-receipts endpoint; its common
            // head is checked by the existing untimed final/cold audit outside.
            let head = if last_wave {
                None
            } else {
                let query_started = Instant::now();
                let head = common_head(nodes, clients, deadline)?;
                timed_head_queries_us += query_started.elapsed().as_micros() as u64;
                Some(head)
            };
            let final_height = observations[offset..]
                .iter()
                .filter_map(|row| match row.receipt {
                    Some(ReceiptState::Finalized { height, .. }) => Some(height),
                    _ => None,
                })
                .max()
                .context("rolling wave has no finalized receipt height")?;
            ensure!(
                head.is_none_or(|head| head.height == final_height),
                "rolling receipt/common-head height mismatch"
            );
            window.verify_completed_wave(parent_height, final_height, WAVE_ROWS)?;
            ensure!(
                observations[offset..].iter().all(|row| {
                    row.complete()
                        && matches!(row.receipt,
                    Some(ReceiptState::Finalized { success: true, height, .. })
                        if height >= window.not_before && height < window.expires)
                }),
                "rolling wave lacks four-node in-window successful receipts"
            );
            waves.push(json!({
                "first_transaction":offset,"transactions":WAVE_ROWS,
                "parent_height":parent_height,"final_height":final_height,
                "not_before_height":window.not_before,"expires_at_height_exclusive":window.expires,
                "all_four_receipts":true,"common_head":head,
                "last_head_checked_by_original_untimed_final_audit":last_wave,
            }));
            offset += WAVE_ROWS;
            if offset == count {
                return observed_duration(observations, false);
            }
            ensure!(
                Instant::now() < deadline,
                "rolling overall deadline before signing"
            );
            parent_height = final_height;
            window = SpendWindow::after(parent_height)?;
            let signing_started = Instant::now();
            append_signed_rows(
                raw,
                observations,
                workload_hash,
                WAVE_ROWS,
                window,
                fee,
                make_signed,
            )?;
            timed_signing_us += signing_started.elapsed().as_micros() as u64;
            ensure!(
                Instant::now() < deadline,
                "rolling overall deadline during signing"
            );
            let prepare_started = Instant::now();
            inputs = prepare_inputs(&mut clients[ingress], &raw[offset..], offset, frame_rows)?;
            timed_preparation_us += prepare_started.elapsed().as_micros() as u64;
            if let Some(sizes) = evidence["application_object_bytes"].as_array_mut() {
                sizes.extend(inputs.iter().map(|input| json!(input.byte_len())));
            }
        }
    })();
    if result.is_err() && start.elapsed() >= timeout && counts.deadline_expirations == 0 {
        counts.deadline_expirations += 1;
    }
    evidence["rolling_waves"] = json!(waves);
    evidence["rolling_timed_signature_generation_us"] = json!(timed_signing_us);
    evidence["rolling_timed_structure_preparation_us"] = json!(timed_preparation_us);
    evidence["rolling_head_query_us"] = json!(timed_head_queries_us);
    evidence["rolling_prepared_transactions"] = json!(raw.len());
    evidence["signed_workload_sha256"] = json!(hex(&workload_hash.clone().finalize()));
    evidence["canonical_signed_input_bytes"] = json!(raw.iter().map(Vec::len).sum::<usize>());
    evidence["rolling_single_clock"] = json!(true);
    result
}

#[test]
#[ignore = "four original processes; legal rolling 65536 NUAS intents and cold economics; run alone"]
fn native_uca_rolling_65536_final_receipts_and_cold_economics() -> Result<()> {
    run_native_uca_load_mode(None, &mut PairedCorpus::default(), None, LoadMode::Rolling)
        .map(|_| ())
}

#[test]
#[ignore = "CPU/GPU/CPU rolling 65536 NUAS business comparison, not byte-identical input; run alone"]
fn native_uca_rolling_65536_signature_cpu_gpu_cpu() -> Result<()> {
    use novovm_exec::pq_signature::MldsaVerificationBackendV1::{Cpu, GpuRequired};
    let mut corpus = PairedCorpus::default();
    let mut reports: Vec<Value> = Vec::new();
    for backend in [Cpu, GpuRequired, Cpu] {
        let report = run_native_uca_load_mode(Some(backend), &mut corpus, None, LoadMode::Rolling)?;
        if let Some(first) = reports.first() {
            for field in [
                "signing_domain",
                "genesis_commitment",
                "initial_root",
                "binary_sha256",
                "aoem_sha256",
                "backend",
                "transactions",
                "wire_frame_rows",
                "query_batch",
                "canonical_body_budget",
                "load_strategy",
                "effective_outstanding_limit",
                "grant_expires_at_height",
                "overall_deadline_seconds",
            ] {
                ensure!(
                    report[field] == first[field],
                    "rolling comparison changed {field}"
                );
            }
            ensure!(
                authority_economics(&report)? == authority_economics(first)?,
                "rolling committed economics differ"
            );
        }
        ensure!(
            report["passed"] == true
                && report["cold_recovery_pass"] == true
                && report["full_fee_state_audited"] == true,
            "rolling comparison validation incomplete"
        );
        reports.push(report);
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-load")
        .join(format!(
            "rolling-paired-{}-{}.json",
            std::process::id(),
            clock_ms()?
        ));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({
            "scope":"one-host four-process rolling-window acceptance; not static 8192-inflight throughput",
            "same_signed_corpus":false,"same_approved_genesis_and_logical_business_workload":true,
            "signature_backend_order":["cpu","gpu_required","cpu"],
            "expiry_and_randomized_signature_bytes_may_differ":true,
            "single_unpaused_wall_per_run":true,"post_first_submit_signing_included":true,
            "all_round_databases_and_journals_retained":true,"rounds":reports,
        }))?,
    )?;
    eprintln!("NUAS rolling paired report={}", path.display());
    Ok(())
}

#[test]
fn native_uca_rolling_window_and_original_funding_contract() -> Result<()> {
    let window = SpendWindow::after(1000)?;
    assert_eq!((window.not_before, window.expires), (1001, 1257));
    window.verify_completed_wave(1000, 1256, WAVE_ROWS)?;
    assert!(window.verify_completed_wave(1000, 1257, WAVE_ROWS).is_err());
    assert!(window.verify_completed_wave(1000, 1000, WAVE_ROWS).is_err());
    assert!(SpendWindow::after(u64::MAX - MAX_SPEND_WINDOW).is_err());
    assert_eq!(LoadMode::Rolling.grant_expiry(65_536)?, 65_793);
    assert!(LoadMode::Rolling.validate_count(65_536, 32).is_ok());
    assert!(LoadMode::Rolling.validate_count(4097, 32).is_err());
    assert!(LoadMode::Rolling.validate_count(65_536, 48).is_err());
    let nonces_per_payer = 65_536 / SENDERS;
    assert_eq!(nonces_per_payer, 4096);
    assert_eq!(nonces_per_payer as u128 * 51, 208_896);
    assert!(nonces_per_payer as u128 * 51 <= FUNDING);
    Ok(())
}

#[test]
fn native_uca_rolling_observer_keeps_signing_and_between_wave_waits() -> Result<()> {
    let row = |first, last| Observation {
        hash: [1; 32],
        first_submit_us: Some(first),
        receipt_query_observed_us: [Some(last); 4],
        receipt: None,
        expected_nonce: 1,
        expected_fee: 1,
    };
    let rows = [row(10, 40), row(1000, 1040)];
    assert_eq!(
        observed_duration(&rows, false)?,
        Duration::from_micros(1030)
    );
    assert_ne!(observed_duration(&rows, false)?, Duration::from_micros(70));
    Ok(())
}

#[test]
fn native_uca_rolling_rows_keep_account_nonce_and_exact_ordered_hash() -> Result<()> {
    // Structural byte fixture only. Actual process tests above obtain signatures
    // from the original AOEM signer; these bytes are never presented as valid.
    use novovm_protocol::uca_spend::{MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES};
    let public_key = vec![7; MLDSA65_PUBLIC_KEY_BYTES];
    let make_signed = |index: usize, window: SpendWindow| -> Result<SignedAssetSpendV2> {
        Ok(SignedAssetSpendV2 {
            intent: AssetSpendIntentV2 {
                asset: 1,
                authority_update: None,
                chain_id: CHAIN,
                genesis: [1; 32],
                payer: account_bytes(index % SENDERS).to_vec(),
                authorization_epoch: 1,
                spend_key_ref: spend_key_ref_v1(&public_key)?,
                recipient: account_bytes(SENDERS).to_vec(),
                amount: 1,
                max_fee: 50,
                nonce: (index / SENDERS) as u64,
                not_before_height: window.not_before,
                expires_at_height: window.expires,
            },
            public_key: public_key.clone(),
            signature: vec![index as u8; MLDSA65_SIGNATURE_BYTES],
        })
    };
    let mut raw = Vec::new();
    let mut observations = Vec::new();
    let mut hash = Sha256::new();
    hash.update(b"novovm/nuas-load/ordered-signed-inputs/v1\0");
    for parent in [0, 3] {
        append_signed_rows(
            &mut raw,
            &mut observations,
            &mut hash,
            SENDERS,
            SpendWindow::after(parent)?,
            2,
            &make_signed,
        )?;
    }
    let mut oracle = Sha256::new();
    oracle.update(b"novovm/nuas-load/ordered-signed-inputs/v1\0");
    for (index, bytes) in raw.iter().enumerate() {
        let signed = SignedAssetSpendV2::decode(bytes)?;
        assert_eq!(signed.intent.nonce, (index / SENDERS) as u64);
        assert_eq!(
            observations[index].expected_nonce,
            (index / SENDERS) as u64 + 1
        );
        oracle.update((bytes.len() as u64).to_be_bytes());
        oracle.update(bytes);
    }
    assert_eq!(hash.clone().finalize(), oracle.finalize());
    let wrong_nonce = |index, window| {
        let mut value = make_signed(index, window)?;
        value.intent.nonce += 1;
        Ok(value)
    };
    assert!(append_signed_rows(
        &mut raw,
        &mut observations,
        &mut hash,
        1,
        SpendWindow::after(5)?,
        2,
        &wrong_nonce,
    )
    .is_err());
    assert_eq!(raw.len(), 2 * SENDERS);
    assert_eq!(observations.len(), raw.len());
    Ok(())
}
