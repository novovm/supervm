//! Dynamic validity windows on the original native load driver; no production
//! protocol or authorization change. Rolling is a 256-row stop/wait correctness
//! fixture. Continuous reuses the original 8192 in-flight ceiling and overlaps
//! replacement signing with SDK polling, without common-head fences. Its signed
//! windows remain 256 heights: an overloaded/underfilled run can expire and must
//! fail, never renew already-submitted bytes or silently widen the contract.
use super::*;
use novovm_protocol::uca_spend::MAX_SPEND_WINDOW;

pub(super) const WAVE_ROWS: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LoadMode {
    Static,
    Rolling,
    Continuous,
}

impl LoadMode {
    pub(super) fn is_rolling(self) -> bool {
        self == Self::Rolling
    }

    pub(super) fn dynamic_windows(self) -> bool {
        self != Self::Static
    }

    pub(super) fn first_rows(self, count: usize) -> usize {
        match self {
            Self::Static => count,
            Self::Rolling => WAVE_ROWS,
            Self::Continuous => count.min(WINDOW),
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Static => "static_signed_corpus_8192_outstanding_limit",
            Self::Rolling => "rolling_256_transaction_common_head_waves",
            Self::Continuous => "continuous_8192_outstanding_receipt_anchored_windows",
        }
    }

    pub(super) fn validate_count(self, count: usize, frame_rows: usize) -> Result<()> {
        ensure!(
            frame_rows > 0
                && frame_rows <= CONFIGURED_BATCH
                && count > 0
                && count.is_multiple_of(frame_rows),
            "load requires whole bounded wire frames"
        );
        if self.is_rolling() {
            ensure!(
                count >= WAVE_ROWS
                    && count.is_multiple_of(WAVE_ROWS)
                    && WAVE_ROWS.is_multiple_of(frame_rows),
                "rolling load requires complete 256-row waves and whole wire frames"
            );
        } else if self == Self::Continuous {
            ensure!(
                count <= WINDOW || WINDOW.is_multiple_of(frame_rows),
                "continuous initial window must contain whole wire frames"
            );
        }
        Ok(())
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
    shape: WorkloadShape,
    make_signed: &impl Fn(usize, SpendWindow) -> Result<SignedAssetSpendV3>,
) -> Result<()> {
    let first = raw.len();
    append_generated_rows(
        raw,
        observations,
        workload_hash,
        (first..first + count).map(|index| make_signed(index, window)),
        window,
        fee,
        shape,
    )
}

pub(super) fn append_generated_rows(
    raw: &mut Vec<Vec<u8>>,
    observations: &mut Vec<Observation>,
    workload_hash: &mut Sha256,
    rows: impl IntoIterator<Item = Result<SignedAssetSpendV3>>,
    window: SpendWindow,
    fee: u128,
    shape: WorkloadShape,
) -> Result<()> {
    ensure!(
        raw.len() == observations.len(),
        "rolling raw/observation index mismatch"
    );
    for signed in rows {
        let index = raw.len();
        let signed = signed?;
        ensure!(
            signed.intent.payer == account_bytes(index % shape.payers)
                && signed.intent.recipient == account_bytes(shape.recipient(index))
                && signed.intent.nonce == (index / shape.payers) as u64
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
            expected_nonce: (index / shape.payers) as u64 + 1,
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
    ensure!(
        frame_rows > 0 && raw.len().is_multiple_of(frame_rows),
        "prepared load must contain whole wire frames"
    );
    let codec = native_decode_limits(frame_rows, true);
    raw.chunks(frame_rows)
        .map(|rows| {
            client.prepare_asset_spend(Arc::new(AssetSpendBatchV3::from_raw(
                rows,
                AssetSpendBatchLimitsV3 {
                    transactions: frame_rows,
                    transaction_bytes: codec.transaction_bytes,
                    body_bytes: codec.body_bytes,
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
    shape: WorkloadShape,
    make_signed: &impl Fn(usize, SpendWindow) -> Result<SignedAssetSpendV3>,
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
                shape,
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

// Independent fixture signatures use the existing signer concurrently; there
// is one bounded generation task, not a product queue or alternate verifier.
pub(super) fn generate_rows(
    first: usize,
    count: usize,
    window: SpendWindow,
    workers: usize,
    make_signed: &(impl Fn(usize, SpendWindow) -> Result<SignedAssetSpendV3> + Sync),
) -> Result<Vec<SignedAssetSpendV3>> {
    ensure!(workers > 0 && workers <= 16, "fixture signer worker bound");
    std::thread::scope(|scope| {
        let chunk = count.div_ceil(workers).max(1);
        let handles = (first..first + count)
            .step_by(chunk)
            .map(|begin| {
                let end = (begin + chunk).min(first + count);
                scope.spawn(move || {
                    (begin..end)
                        .map(|index| make_signed(index, window))
                        .collect::<Result<Vec<_>>>()
                })
            })
            .collect::<Vec<_>>();
        let mut rows = Vec::with_capacity(count);
        for handle in handles {
            rows.extend(
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("fixture signer panicked"))??,
            );
        }
        Ok(rows)
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn drive_continuous(
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
    shape: WorkloadShape,
    make_signed: &(impl Fn(usize, SpendWindow) -> Result<SignedAssetSpendV3> + Sync),
    evidence: &mut Value,
) -> Result<Duration> {
    let workers = env_count("NOVOVM_NATIVE_LOAD_SIGNERS", 4, 16)?;
    let start = Instant::now();
    let mut driver = LoadDriver::new(&inputs, observations.len(), start)?;
    let mut signing_us = 0u64;
    let mut preparation_us = 0u64;
    let mut generator_starved_us = 0u64;
    let mut no_outstanding_us = 0u64;
    let mut outstanding_row_us = 0u128;
    let mut intervals = vec![(0, raw.len(), SpendWindow::after(0)?)];
    // Never append a partial frame and then count it as frame_rows in the
    // original driver. Larger APFL inputs replenish one or more whole frames;
    // this changes preparation granularity, not the 8192 outstanding ceiling.
    let generation_rows = WAVE_ROWS.div_ceil(frame_rows) * frame_rows;
    let result = std::thread::scope(|scope| -> Result<Duration> {
        let mut pending = None;
        loop {
            let turn = Instant::now();
            // A four-node observation frees the original WINDOW budget, but a
            // whole window does not have to drain before its replacement begins.
            if pending.is_none() && raw.len() < count {
                let generate = generation_rows.min(count - raw.len());
                if raw.len() + generate - driver.complete <= WINDOW {
                    let first = raw.len();
                    let parent = *driver.finalized_heights.iter().min().unwrap();
                    let window = SpendWindow::after(parent)?;
                    pending = Some((
                        first,
                        window,
                        scope.spawn(move || {
                            let generated_at = Instant::now();
                            let rows =
                                generate_rows(first, generate, window, workers, make_signed)?;
                            Ok::<_, anyhow::Error>((
                                rows,
                                generated_at.elapsed().as_micros() as u64,
                            ))
                        }),
                    ));
                }
            }
            if pending
                .as_ref()
                .is_some_and(|(_, _, task)| task.is_finished())
            {
                let (first, window, task) = pending.take().unwrap();
                let (rows, elapsed) = task
                    .join()
                    .map_err(|_| anyhow::anyhow!("continuous fixture signer panicked"))??;
                signing_us += elapsed;
                ensure!(raw.len() == first, "continuous signer reordered input");
                let prepare = Instant::now();
                append_generated_rows(
                    raw,
                    observations,
                    workload_hash,
                    rows.into_iter().map(Ok),
                    window,
                    fee,
                    shape,
                )?;
                let prepared =
                    prepare_inputs(&mut clients[ingress], &raw[first..], first, frame_rows)?;
                driver
                    .ids
                    .extend(prepared.iter().map(PreparedApplicationInput::id));
                if let Some(sizes) = evidence["application_object_bytes"].as_array_mut() {
                    sizes.extend(prepared.iter().map(|input| json!(input.byte_len())));
                }
                inputs.extend(prepared);
                preparation_us += prepare.elapsed().as_micros() as u64;
                intervals.push((first, raw.len(), window));
            }
            let awaiting_generator = driver.next_batch == inputs.len() && raw.len() < count;
            let outstanding = driver.dispatched - driver.complete;
            driver.poll(
                nodes,
                clients,
                ingress,
                &inputs,
                frame_rows,
                observations,
                counts,
                timeout,
            )?;
            let elapsed = turn.elapsed().as_micros() as u64;
            outstanding_row_us += u128::from(elapsed) * outstanding as u128;
            if awaiting_generator {
                generator_starved_us += elapsed;
            }
            if outstanding == 0 {
                no_outstanding_us += elapsed;
            }
            if driver.finished(count, inputs.len(), false) {
                break;
            }
            std::thread::yield_now();
        }
        for &(first, end, window) in &intervals {
            ensure!(
                observations[first..end].iter().all(|row| {
                    row.complete()
                        && matches!(row.receipt,
                    Some(ReceiptState::Finalized { success: true, height, .. })
                    if height >= window.not_before && height < window.expires)
                }),
                "continuous signed workload expired or lacks four successful receipts"
            );
        }
        observed_duration(observations, false)
    });
    evidence["continuous_signer_workers"] = json!(workers);
    evidence["continuous_generation_batch_rows"] = json!(generation_rows);
    evidence["continuous_timed_generation_task_us"] = json!(signing_us);
    evidence["continuous_timed_structure_preparation_us"] = json!(preparation_us);
    evidence["continuous_generator_starved_poll_us"] = json!(generator_starved_us);
    evidence["continuous_no_outstanding_poll_us"] = json!(no_outstanding_us);
    evidence["continuous_outstanding_row_poll_us"] = json!(outstanding_row_us);
    evidence["continuous_generation_windows"] = json!(intervals
        .iter()
        .map(|(first, end, window)| json!({
            "first_transaction":first,"transactions":end-first,
            "not_before_height":window.not_before,"expires_at_height_exclusive":window.expires,
        }))
        .collect::<Vec<_>>());
    evidence["continuous_phase_timing_scope"] = json!("generation task wall overlaps original SDK polling; phase times are not additive and never subtracted from TPS; poll starvation excludes yield scheduling");
    evidence["continuous_prepared_transactions"] = json!(raw.len());
    evidence["continuous_single_clock"] = json!(true);
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
    run_dynamic_pair(LoadMode::Rolling)
}

#[test]
#[ignore = "four original processes; continuous bounded 65536 legal signed load and cold economics; run alone"]
fn native_asset_continuous_65536_final_receipts_and_cold_economics() -> Result<()> {
    run_native_uca_load_mode(
        None,
        &mut PairedCorpus::default(),
        None,
        LoadMode::Continuous,
    )
    .map(|_| ())
}

#[test]
#[ignore = "CPU/GPU/CPU continuous 65536 load, same logical business not byte-identical dynamic windows; run alone"]
fn native_asset_continuous_65536_signature_cpu_gpu_cpu() -> Result<()> {
    run_dynamic_pair(LoadMode::Continuous)
}

fn run_dynamic_pair(mode: LoadMode) -> Result<()> {
    use novovm_exec::pq_signature::MldsaVerificationBackendV1::{Cpu, GpuRequired};
    let mut corpus = PairedCorpus::default();
    let mut reports: Vec<Value> = Vec::new();
    for backend in [Cpu, GpuRequired, Cpu] {
        let report = run_native_uca_load_mode(Some(backend), &mut corpus, None, mode)?;
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
                "continuous_signer_workers",
                "spend_authority",
                "restricted_grants",
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
            "{}-paired-{}-{}.json",
            mode.label(),
            std::process::id(),
            clock_ms()?
        ));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({
            "scope":mode.label(),
            "same_signed_corpus":false,"same_approved_genesis_and_logical_business_workload":true,
            "signature_backend_order":["cpu","gpu_required","cpu"],
            "expiry_and_randomized_signature_bytes_may_differ":true,
            "single_unpaused_wall_per_run":true,"post_first_submit_signing_included":true,
            "all_round_databases_and_journals_retained":true,"rounds":reports,
        }))?,
    )?;
    eprintln!("NUAS dynamic-window paired report={}", path.display());
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
    assert!(LoadMode::Rolling.validate_count(65_536, 32).is_ok());
    assert!(LoadMode::Rolling.validate_count(4097, 32).is_err());
    assert!(LoadMode::Rolling.validate_count(65_536, 48).is_err());
    assert_eq!(LoadMode::Continuous.first_rows(65_536), WINDOW);
    assert_eq!(LoadMode::Continuous.first_rows(1024), 1024);
    assert!(LoadMode::Continuous.validate_count(65_536, 32).is_ok());
    assert!(LoadMode::Continuous.validate_count(65_536, 1024).is_ok());
    assert!(LoadMode::Continuous.validate_count(65_536, 0).is_err());
    assert!(LoadMode::Continuous.validate_count(65_536, 2048).is_err());
    assert!(LoadMode::Continuous.validate_count(9000, 1000).is_err());
    assert!(LoadMode::Rolling.validate_count(65_536, 1024).is_err());
    assert!(LoadMode::Static.validate_count(65_536, 1024).is_ok());
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
    let make_signed = |index: usize, window: SpendWindow| -> Result<SignedAssetSpendV3> {
        Ok(SignedAssetSpendV3 {
            intent: AssetSpendIntentV3 {
                authority: AssetSpendAuthorityV3::Controller,
                asset: 1,
                authority_update: None,
                recipient_creation: None,
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
    let parallel = generate_rows(0, 2 * SENDERS, SpendWindow::after(0)?, 4, &make_signed)?;
    for (index, signed) in parallel.iter().enumerate() {
        assert!(signed == &make_signed(index, SpendWindow::after(0)?)?);
    }
    assert!(generate_rows(0, SENDERS, SpendWindow::after(0)?, 0, &make_signed).is_err());
    let failed_signer = |index, window| {
        ensure!(index != SENDERS, "injected fixture signing failure");
        make_signed(index, window)
    };
    assert!(generate_rows(0, 2 * SENDERS, SpendWindow::after(0)?, 4, &failed_signer).is_err());
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
            WorkloadShape::hotspot(),
            &make_signed,
        )?;
    }
    let mut oracle = Sha256::new();
    oracle.update(b"novovm/nuas-load/ordered-signed-inputs/v1\0");
    for (index, bytes) in raw.iter().enumerate() {
        let signed = SignedAssetSpendV3::decode(bytes)?;
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
        WorkloadShape::hotspot(),
        &wrong_nonce,
    )
    .is_err());
    assert_eq!(raw.len(), 2 * SENDERS);
    assert_eq!(observations.len(), raw.len());
    Ok(())
}

#[test]
fn native_asset_1024_apfl_frame_sizes_and_lossless_roundtrip() -> Result<()> {
    use crate::native_pipeline::consensus::transport::{self, Message, TransactionsScope};
    use novovm_protocol::uca_spend::{MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES};

    // Structural sizing only: patterned signature bytes are deliberately not
    // presented as cryptographically valid. The process load uses AOEM signing.
    let codec = native_decode_limits(CONFIGURED_BATCH, true);
    assert_eq!(codec.transactions, 1024);
    assert_eq!(codec.body_bytes, 8 * 1024 * 1024);
    let mut reports = Vec::new();
    for distinct_keys in [1, SENDERS, CONFIGURED_BATCH] {
        let raw = (0..CONFIGURED_BATCH)
            .map(|index| {
                let mut public_key = vec![7; MLDSA65_PUBLIC_KEY_BYTES];
                public_key[..8].copy_from_slice(&((index % distinct_keys) as u64).to_be_bytes());
                SignedAssetSpendV3 {
                    intent: AssetSpendIntentV3 {
                        authority: AssetSpendAuthorityV3::Controller,
                        chain_id: CHAIN,
                        genesis: [1; 32],
                        payer: account_bytes(index % SENDERS).to_vec(),
                        recipient: account_bytes(SENDERS).to_vec(),
                        asset: 1,
                        amount: 1,
                        max_fee: 50,
                        nonce: (index / SENDERS) as u64,
                        authorization_epoch: 1,
                        spend_key_ref: spend_key_ref_v1(&public_key)?,
                        not_before_height: 1,
                        expires_at_height: 257,
                        authority_update: None,
                        recipient_creation: None,
                    },
                    public_key,
                    signature: vec![index as u8; MLDSA65_SIGNATURE_BYTES],
                }
                .encode()
                .map_err(anyhow::Error::from)
            })
            .collect::<Result<Vec<_>>>()?;
        let limits = AssetSpendBatchLimitsV3 {
            transactions: codec.transactions,
            transaction_bytes: codec.transaction_bytes,
            body_bytes: codec.body_bytes,
        };
        let batch = Arc::new(AssetSpendBatchV3::from_raw(&raw, limits)?);
        let apfl_bytes = batch.encode()?.len();
        let message = Message::AssetTransactions {
            scope: TransactionsScope {
                chain_id: CHAIN,
                genesis: [1; 32],
                protocol: [2; 32],
                epoch: 1,
                validator_set_hash: [3; 32],
                session: [4; 32],
                sequence: 1,
            },
            batch,
        };
        let wire = transport::encode(&message, codec)?;
        let Message::AssetTransactions { batch, .. } = transport::decode(&wire, codec)? else {
            bail!("1024-row asset APFL input changed message kind");
        };
        assert_eq!(batch.len(), CONFIGURED_BATCH);
        assert_eq!(
            batch.canonical_bytes(),
            raw.iter().map(Vec::len).sum::<usize>()
        );
        for (index, expected) in raw.iter().enumerate() {
            assert_eq!(&batch.raw_transaction(index)?, expected);
        }
        let mut sizes = apfl_frame_sizes(&raw, CONFIGURED_BATCH)?;
        assert_eq!(sizes.len(), 1);
        assert_eq!(sizes[0]["apfl_encoded_bytes"], json!(apfl_bytes));
        sizes[0]["distinct_public_keys"] = json!(distinct_keys);
        sizes[0]["native_application_object_bytes"] = json!(wire.len());
        reports.push(sizes.remove(0));
    }
    println!(
        "{}",
        json!({
            "scope":"actual original codec structural sizing and exact byte roundtrip, not signature or TPS acceptance",
            "transactions":CONFIGURED_BATCH,"expanded_budget":codec.body_bytes,
            "message_budget":codec.message_bytes,"frames":reports,
        })
    );
    Ok(())
}
