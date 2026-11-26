//! NUAP consumes the existing signature permit, immutable-tree I/O and pool.
//! No application executor, grant database, asset nonce or fee lives here.
use super::*;
use crate::native_pipeline::{
    business::uca_application::{self, ApplicationGrantAccessV1},
    persistence::io::NodeReadReply,
    pipeline::{CandidatePipeline, SignatureAdmissionOutput},
    state::frontier::{BulkCapture, CaptureStep, DeclaredAccess},
};
use novovm_protocol::uca_application::SignedApplicationGrantV1;

pub(super) const MAX_ROWS: usize = novovm_exec::pq_signature::MLDSA_VERIFY_BATCH_MAX_ITEMS;
const ACCESS_KEYS_PER_ROW: usize = 35;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ApplicationPosition {
    pub(super) grant_id: Hash,
    pub(super) expected_revision: u64,
}

pub(crate) fn is_application_context(
    context: &crate::native_pipeline::execution::plan::BatchContext,
) -> bool {
    crate::native_pipeline::business::uca_registration::is_profile(context)
        || (context.business_program == uca_application::application_program_id_v1()
            && context.semantic_version == uca_application::APPLICATION_SEMANTIC_VERSION_V1
            && context.receipt_codec == uca_application::application_receipt_codec_v1())
}

/// Dispatch only; NUAP shape and signatures are checked by the original owner.
pub(super) fn raw_input(v4: bool, raw: Vec<u8>) -> AdmissionInput {
    if v4 && raw.starts_with(b"NUAP") {
        AdmissionInput::Application(raw)
    } else {
        AdmissionInput::Raw(raw)
    }
}

pub(super) fn position(admitted: &AdmittedTransfer) -> Result<ApplicationPosition> {
    let intent = admitted
        .checked_application()
        .context("application fact missing")?
        .intent();
    Ok(ApplicationPosition {
        grant_id: intent.selector.grant_id()?,
        expected_revision: intent.expected_revision,
    })
}

pub(super) fn receipt_json(
    receipt: &novovm_protocol::uca_application::ApplicationGrantReceiptV1,
    point: ParentPoint,
) -> Value {
    json!({"tx_hash":hex(&receipt.command_id), "state":"finalized_application",
        "finalized":true, "executed":true, "proof_verified":false,
        "finality_kind":"BFT_durable", "grant_id":hex(&receipt.grant_id),
        "previous_revision":receipt.previous_revision, "revision":receipt.revision,
        "revoked":receipt.revoked, "block_height":point.height,
        "not_before_height":receipt.not_before_height, "expires_at_height":receipt.expires_at_height,
        "state_root":hex(&point.state_root), "block_hash":hex(&point.block_hash)})
}

/// One bounded read-only capture, always anchored by the controller, not input.
pub(super) struct ApplicationRead {
    access: ApplicationGrantAccessV1,
    capture: Option<BulkCapture>,
    ticket: Option<IoTicket<NodeReadReply>>,
    parent: ParentPoint,
}

impl ApplicationRead {
    pub(super) fn new(
        pipeline: &CandidatePipeline,
        parent: ParentPoint,
        input: &AdmissionInput,
        admitted: &AdmittedTransfer,
    ) -> Result<Self> {
        ensure!(
            pipeline.owns_admitted(admitted),
            "foreign application verifier owner"
        );
        let checked = reattach(input, admitted)?;
        let access = ApplicationGrantAccessV1::new(checked.intent().selector.clone())?;
        let declarations: Vec<_> = access
            .declarations()
            .iter()
            .map(|d| DeclaredAccess {
                key: d.key.clone(),
                may_put: false,
                may_delete: false,
            })
            .collect();
        ensure!(
            parent.state_root != [0; 32],
            "published application root missing"
        );
        let capture =
            BulkCapture::new(parent.state_root, &declarations, pipeline.capture_budget())?;
        Ok(Self {
            access,
            capture: Some(capture),
            ticket: None,
            parent,
        })
    }

    pub(super) fn poll(
        &mut self,
        pipeline: &CandidatePipeline,
        input: &AdmissionInput,
        admitted: &AdmittedTransfer,
    ) -> Result<Option<()>> {
        let capture = self
            .capture
            .as_mut()
            .context("application capture consumed")?;
        if let Some(ticket) = self.ticket.as_mut() {
            let Some(nodes) = ticket.try_take()? else {
                return Ok(None);
            };
            capture.accept(nodes)?;
            self.ticket = None;
            return Ok(None);
        }
        match capture.advance(64)? {
            CaptureStep::More => Ok(None),
            CaptureStep::NeedRead => {
                let nodes = capture
                    .next_request()?
                    .context("application read nodes missing")?;
                self.ticket = pipeline.try_read_state_nodes(nodes)?;
                Ok(None)
            }
            CaptureStep::Complete => {
                let input_state = self.capture.take().expect("application capture").finish()?;
                let native_registration = pipeline
                    .business_schedule
                    .as_ref()
                    .is_some_and(|schedule| schedule.is_active_at(self.parent.height));
                let state = self.access.read_under_rules(
                    &input_state,
                    self.parent.state_root,
                    self.parent.height,
                    native_registration,
                )?;
                let checked = reattach(input, admitted)?;
                // Reuse the exact transition checker; discard proposed changes.
                // This read-only frontier cannot stage or publish those changes.
                self.access.transition(
                    &state,
                    &checked,
                    self.parent.state_root,
                    self.parent
                        .height
                        .checked_add(1)
                        .context("application height exhausted")?,
                )?;
                Ok(Some(()))
            }
        }
    }
}

fn reattach(
    input: &AdmissionInput,
    admitted: &AdmittedTransfer,
) -> Result<novovm_exec::uca_application::CheckedApplicationGrantV1> {
    let AdmissionInput::Application(raw) = input else {
        anyhow::bail!("application input required")
    };
    admitted
        .checked_application()
        .context("application signature fact required")?
        .reattach(SignedApplicationGrantV1::decode(raw)?)
}

pub(super) struct ApplicationWork {
    // Keep the SAME ingress byte/slot permit throughout authority I/O.
    _output: SignatureAdmissionOutput,
    rows: VecDeque<Result<(AdmissionInput, AdmittedTransfer)>>,
    pub(super) results: VecDeque<Result<Hash>>,
    read: Option<ApplicationRead>,
    parent: Option<ParentPoint>,
    pool_wait: Option<(usize, usize, usize)>,
}

impl ApplicationWork {
    pub(super) fn new(mut output: SignatureAdmissionOutput) -> Self {
        let mut rows = VecDeque::with_capacity(output.len());
        while let Some(row) = output.pop_admitted() {
            rows.push_back(row.result.map(|checked| (row.input, checked)));
        }
        Self {
            _output: output,
            rows,
            results: VecDeque::new(),
            read: None,
            parent: None,
            pool_wait: None,
        }
    }
}

pub(super) struct ApplicationSelection {
    position: Position,
    hashes: VecDeque<Hash>,
    selected: Vec<Hash>,
    read: Option<ApplicationRead>,
}

/// References only: missing facts or stale authority still fail in capture.
fn selection_window(
    order: &VecDeque<Hash>,
    pending: &BTreeMap<Hash, Pending>,
    batch_size: usize,
    key_limit: usize,
) -> VecDeque<Hash> {
    let mut hashes = VecDeque::new();
    let mut bytes = 0;
    let mut access_keys = 0;
    for hash in order {
        let Some(entry) = pending.get(hash) else {
            continue;
        };
        if !matches!(entry.position, PendingPosition::Application(_)) {
            continue;
        }
        let Some(row_bytes) = entry.selection_input_bytes() else {
            continue;
        };
        if hashes.len() == batch_size.min(MAX_ROWS)
            || row_bytes > super::super::uca_body_byte_limit(batch_size).saturating_sub(bytes)
            || ACCESS_KEYS_PER_ROW > key_limit.saturating_sub(access_keys)
        {
            break;
        }
        bytes += row_bytes;
        access_keys += ACCESS_KEYS_PER_ROW;
        hashes.push_back(*hash);
    }
    hashes
}

impl RpcLifecycle {
    pub(super) fn trim_recent_receipts(&mut self) {
        while self.recent.len() > RECENT_RECEIPTS {
            let hash = self.recent.pop_front().expect("recent receipt");
            self.receipts.remove(&hash);
            self.application_receipts.remove(&hash);
        }
    }
    pub(crate) fn supports_application(&self) -> bool {
        is_application_context(
            &self
                .node
                .business_context(self.node.controller.context().height),
        )
    }

    pub(super) fn poll_application_admission(
        &mut self,
        work: &mut ApplicationWork,
        broadcast: bool,
    ) -> Result<bool> {
        let parent = self.node.controller.parent();
        if work.parent != Some(parent) {
            work.parent = Some(parent);
            work.read = None;
            work.pool_wait = None;
        }
        if work.rows.is_empty() {
            return Ok(true);
        }
        let pool = (
            self.pending.len(),
            self.pending_bytes,
            self.pending_retention.bytes,
        );
        if work.pool_wait == Some(pool) {
            self.ingress_batch_boundary = true;
            return Ok(false);
        }
        work.pool_wait = None;
        let result = match work.rows.front().expect("application row") {
            Err(_) => Some(Err(anyhow::anyhow!("application signature rejected"))),
            Ok((input, admitted)) => {
                let result = (|| {
                    ensure!(
                        self.supports_application() && self.projection_error.is_none(),
                        "application rules unavailable"
                    );
                    let intent = admitted
                        .checked_application()
                        .context("application fact missing")?
                        .intent();
                    ensure!(
                        intent.selector.chain_id == self.node.template.chain_id
                            && intent.selector.genesis
                                == self.node.template.genesis_config_commitment,
                        "application domain mismatch"
                    );
                    if work.read.is_none() {
                        work.read = Some(ApplicationRead::new(
                            &self.node.pipeline,
                            parent,
                            input,
                            admitted,
                        )?);
                    }
                    work.read.as_mut().expect("application read").poll(
                        &self.node.pipeline,
                        input,
                        admitted,
                    )
                })();
                match result {
                    Ok(None) => None,
                    Ok(Some(())) => Some(Ok(())),
                    Err(e) => Some(Err(e)),
                }
            }
        };
        let Some(result) = result else {
            return Ok(false);
        };
        work.read = None;
        if result.is_ok() {
            let Ok((input, admitted)) = work.rows.front().expect("application row") else {
                unreachable!()
            };
            let metadata = admitted.retained_metadata_bytes();
            let bytes = input.encoded_len()?;
            if self.pending.len() >= MAX_POOL
                || bytes > MAX_POOL_BYTES.saturating_sub(self.pending_bytes)
                || crate::native_pipeline::ingress::batch::admission_retained_bytes(
                    std::slice::from_ref(input),
                )?
                .checked_add(metadata)
                .is_none_or(|n| n > MAX_POOL_BYTES.saturating_sub(self.pending_retention.bytes))
            {
                work.pool_wait = Some(pool);
                self.ingress_batch_boundary = true;
                return Ok(false);
            }
        }
        let row = work.rows.pop_front().expect("application row");
        let result = result.and_then(|()| {
            let (input, admitted) = row?;
            self.admit_application_checked(input, admitted, broadcast)
        });
        work.results.push_back(result);
        self.admission.made_progress = true;
        Ok(work.rows.is_empty())
    }

    fn admit_application_checked(
        &mut self,
        input: AdmissionInput,
        admitted: AdmittedTransfer,
        broadcast: bool,
    ) -> Result<Hash> {
        ensure!(
            self.supports_application() && self.node.pipeline.owns_admitted(&admitted),
            "application owner/profile mismatch"
        );
        let checked = reattach(&input, &admitted)?;
        let key = position(&admitted)?;
        let hash = uca_application::application_command_id(checked.intent())?;
        if let Some(existing) = self.pending.get(&hash) {
            ensure!(
                matches!((&existing.input, &input), (AdmissionInput::Application(a), AdmissionInput::Application(b)) if a == b),
                "application duplicate signed bytes mismatch"
            );
            if broadcast {
                self.queue_gossip(hash);
            }
            return Ok(hash);
        }
        ensure!(
            !self.pending.values().any(|entry| matches!(entry.position,
            PendingPosition::Application(p) if p.grant_id == key.grant_id)),
            "application grant already pending"
        );
        let canonical_len = input.encoded_len()?;
        ensure!(
            self.pending.len() < MAX_POOL
                && canonical_len <= MAX_POOL_BYTES.saturating_sub(self.pending_bytes),
            "application pool backpressure"
        );
        self.pending_retention.reserve(
            &input,
            admitted.retained_metadata_bytes(),
            MAX_POOL_BYTES,
        )?;
        self.pending_bytes += canonical_len;
        self.pending.insert(
            hash,
            Pending {
                input,
                admitted: Some(admitted),
                canonical_len,
                position: PendingPosition::Application(key),
                gossip_missing: 0,
            },
        );
        if self.application_receipts.remove(&hash).is_some() {
            self.recent.retain(|previous| *previous != hash);
        }
        self.order.push_back(hash);
        self.pool_revision = self.pool_revision.wrapping_add(1);
        if broadcast {
            self.queue_gossip(hash);
        }
        Ok(hash)
    }

    pub(super) fn selected_application_raw(&self, hashes: &[Hash]) -> Result<Vec<Vec<u8>>> {
        hashes
            .iter()
            .map(|hash| {
                match &self
                    .pending
                    .get(hash)
                    .context("application row missing")?
                    .input
                {
                    AdmissionInput::Application(raw) => Ok(raw.clone()),
                    _ => anyhow::bail!("mixed application and asset candidate"),
                }
            })
            .collect()
    }

    /// Application candidates use only the actual published parent. No asset
    /// nonce overlay is reinterpreted as grant revisions for private lookahead.
    pub(super) fn poll_application_selection(&mut self) -> Result<bool> {
        if !self.supports_application() {
            return Ok(false);
        }
        let has_application = self
            .order
            .iter()
            .filter_map(|h| self.pending.get(h))
            .any(|p| matches!(p.position, PendingPosition::Application(_)));
        if !has_application && self.application_selection.is_none() {
            return Ok(false);
        }
        if self.application_selection.is_none()
            && !self.prefer_application
            && self
                .pending
                .values()
                .any(|entry| entry.asset_position().is_some())
        {
            return Ok(false);
        }
        let parent = self.node.controller.parent();
        let position = Position {
            height: parent
                .height
                .checked_add(1)
                .context("application height exhausted")?,
            round: self.node.controller.round(),
            parent,
            source: None,
        };
        if self.offered == Some(position) {
            return Ok(true);
        }
        if self
            .application_selection
            .as_ref()
            .is_none_or(|s| s.position != position)
        {
            let key_limit = self
                .node
                .pipeline
                .capture_budget()
                .keys
                .min(ASSET_PLAN_ACCESS_KEYS)
                .min(crate::native_pipeline::state::tree::MAX_BATCH_READ_KEYS);
            let hashes = selection_window(&self.order, &self.pending, self.batch_size, key_limit);
            self.application_selection = Some(ApplicationSelection {
                position,
                hashes,
                selected: Vec::new(),
                read: None,
            });
        }
        let mut selection = self
            .application_selection
            .take()
            .expect("application selection");
        if let Some(hash) = selection.hashes.front().copied() {
            let result = (|| {
                let entry = self
                    .pending
                    .get(&hash)
                    .context("application selection row retired")?;
                let admitted = entry
                    .admitted
                    .as_ref()
                    .context("application selection fact missing")?;
                if selection.read.is_none() {
                    selection.read = Some(ApplicationRead::new(
                        &self.node.pipeline,
                        parent,
                        &entry.input,
                        admitted,
                    )?);
                }
                selection
                    .read
                    .as_mut()
                    .expect("application selection read")
                    .poll(&self.node.pipeline, &entry.input, admitted)
            })();
            match result {
                Ok(None) => {
                    self.application_selection = Some(selection);
                    return Ok(true);
                }
                Ok(Some(())) => selection.selected.push(hash),
                Err(_) => {
                    self.remove_application_pending(hash);
                    self.application_receipts.insert(
                        hash,
                        json!({"tx_hash":hex(&hash),
                        "state":"rejected_application", "finalized":false, "proof_verified":false}),
                    );
                    self.recent.push_back(hash);
                    self.trim_recent_receipts();
                    self.gossip_rejected += 1;
                }
            }
            selection.hashes.pop_front();
            selection.read = None;
            self.application_selection = Some(selection);
            return Ok(true);
        }
        selection
            .selected
            .retain(|hash| self.pending.contains_key(hash));
        if selection.selected.is_empty() {
            return Ok(false);
        }
        let mut context = self.node.business_context(position.height);
        context.slot = position.height;
        context.timestamp_unix_ms = context
            .timestamp_unix_ms
            .checked_add(position.height)
            .context("application timestamp overflow")?;
        context.parent_height = parent.height;
        context.parent_block_hash = parent.block_hash;
        context.parent_state_root = parent.state_root;
        context.parent_receipt_root = parent.receipt_batch_commitment;
        context.parent_state_version = parent.state_version;
        let message = Arc::new(Message::Body {
            context,
            raw_transactions: self.selected_application_raw(&selection.selected)?,
        });
        if self.node.controller.offer(&message)? {
            self.offer = Some(LocalOffer {
                position,
                message,
                hashes: selection.selected,
            });
            self.offered = Some(position);
            self.prefer_application = false;
            self.ingress_batch_boundary = false;
        } else {
            self.application_selection = Some(selection);
        }
        Ok(true)
    }

    pub(super) fn remove_application_pending(&mut self, hash: Hash) {
        if let Some(entry) = self.pending.remove(&hash) {
            debug_assert!(matches!(entry.position, PendingPosition::Application(_)));
            self.gossip_repair.remove(entry.gossip_missing);
            self.pending_bytes -= entry.canonical_len;
            self.pending_retention
                .release(&entry.input, entry.retained_metadata_bytes());
            self.pool_revision = self.pool_revision.wrapping_add(1);
        }
        self.order.retain(|h| *h != hash);
        self.gossip_order.retain(|h| *h != hash);
        self.gossip_queued.remove(&hash);
    }
}

#[cfg(test)]
mod tests;
