//! Unstaged successor input. Dropping this value reserves no slot and leaves no
//! candidate input marker. Only the live-checked finishing owner may stage it.

use super::*;

enum InputDocument {
    Reference(Box<state_records::PreparedReferenceDocument>),
    Records(Box<state_records::PreparedDocument>),
}

pub(super) struct CapturedInput {
    pub(super) parent: [u8; 32],
    pub(super) genesis: [u8; 32],
    parent_output: [u8; 32],
    document: InputDocument,
}

pub(super) struct NewInput {
    pub(super) descriptor: Descriptor,
    pub(super) verified: VerifiedInput,
    pub(super) captured: CapturedInput,
}

/// None means an explicitly existing reservation, not an error fallback. Its
/// original create/begin routines retain all old bytes and recovery behavior.
pub(super) fn capture_locked(
    workspace: &mut WorkspaceStore,
    plan: &NovNativeCandidateExecutionPlanV1,
    parent_id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<Option<NewInput>> {
    plan.validate()?;
    if plan.context.chain_id != workspace.chain_id
        || plan.protocol_config_commitment != workspace.protocol
    {
        bail!("captured candidate plan domain differs from workspace");
    }
    let parent =
        live_parent::capture_finalized_parent_view_locked(workspace, parent_id, genesis, params)?;
    if parent.successor_plan_locked(workspace, plan.context, plan.raw_txs.clone(), params)? != *plan
    {
        bail!("captured successor input differs from live finalized parent");
    }
    let id = workspace_id(&workspace.scope, &plan.plan_commitment);
    if workspace.graph.get(&workspace.key(b'g', &id))?.is_some() {
        bail!("retired candidate workspace cannot be revived");
    }
    if workspace.catalog()?.iter().any(|(_, input)| input.id == id) {
        return Ok(None);
    }
    let parent_output = parent.output_digest();
    if plan_contains_only_transfers(plan)? {
        if let Some(mut payload) = parent.light_payload(plan)? {
            validate_light_payload(&payload, workspace)?;
            let prepared = state_records::prepare_reference(
                workspace,
                &payload,
                &["finalized_parent", "store"],
                payload
                    .record_state
                    .as_ref()
                    .context("captured parent reference missing")?,
            );
            match prepared {
                Ok(prepared) => {
                    payload.record_state = Some(prepared.state().clone());
                    let descriptor = describe_light(
                        &payload,
                        &prepared.bytes,
                        prepared.state(),
                        &workspace.scope,
                    )?;
                    return Ok(Some(NewInput {
                        descriptor,
                        verified: VerifiedInput::Light(Box::new(payload)),
                        captured: CapturedInput {
                            parent: parent_id,
                            genesis,
                            parent_output,
                            document: InputDocument::Reference(Box::new(prepared)),
                        },
                    }));
                }
                Err(error) if error.is::<state_records::ReferenceInputTooLarge>() => {}
                Err(error) => return Err(error),
            }
        }
    }
    let payload = Payload {
        schema: SCHEMA.into(),
        plan: plan.clone(),
        parent_block: None,
        parent_snapshot: None,
        genesis: None,
        finalized_parent: Some(parent.cold_snapshot(workspace, params)?),
        // This reference names only already durable parent records. Do not
        // pretend a new cold import's unstaged nodes are readable by the worker.
        record_state: parent.record_state().cloned(),
    };
    validate_payload(&payload, workspace)?;
    let store = payload.parent_store()?;
    let path = state_records::payload_path(&payload)?;
    let parent_ref = payload
        .record_state
        .as_ref()
        .map(|reference| (reference, store));
    let prepared = if payload.root_codec_profile()?
        == crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1
    {
        state_records::prepare_record_profile(workspace, &payload, &path, store, parent_ref, None)?
    } else {
        state_records::prepare(workspace, &payload, &path, store, parent_ref)?
    };
    let descriptor = describe(&payload, &prepared.bytes, &workspace.scope)?;
    Ok(Some(NewInput {
        descriptor,
        verified: VerifiedInput::Cold(Box::new(payload)),
        captured: CapturedInput {
            parent: parent_id,
            genesis,
            parent_output,
            document: InputDocument::Records(Box::new(prepared)),
        },
    }))
}

pub(super) fn validate_and_stage(
    workspace: &mut WorkspaceStore,
    input: &Descriptor,
    captured: &CapturedInput,
    verified: &VerifiedInput,
    params: &serde_json::Value,
) -> Result<()> {
    let plan = match verified {
        VerifiedInput::Cold(payload) => &payload.plan,
        VerifiedInput::Light(payload) => &payload.plan,
    };
    let current = live_parent::capture_finalized_parent_view_locked(
        workspace,
        captured.parent,
        captured.genesis,
        params,
    )?;
    if current.output_digest() != captured.parent_output
        || current.successor_plan_locked(workspace, plan.context, plan.raw_txs.clone(), params)?
            != *plan
    {
        bail!("captured candidate parent changed before input staging");
    }
    let expected = match (&captured.document, verified) {
        (InputDocument::Reference(document), VerifiedInput::Light(payload)) => {
            validate_light_payload(payload, workspace)?;
            describe_light(payload, &document.bytes, document.state(), &workspace.scope)?
        }
        (InputDocument::Records(document), VerifiedInput::Cold(payload)) => {
            validate_payload(payload, workspace)?;
            describe(payload, &document.bytes, &workspace.scope)?
        }
        _ => bail!("captured input document and verified payload disagree"),
    };
    if expected != *input {
        bail!("captured candidate input descriptor changed before staging");
    }
    stage(workspace, input, captured)
}

fn stage(
    workspace: &mut WorkspaceStore,
    input: &Descriptor,
    captured: &CapturedInput,
) -> Result<()> {
    match &captured.document {
        InputDocument::Reference(document) => {
            stage_payload_bytes(
                workspace,
                input.clone(),
                &document.bytes,
                |workspace| state_records::persist_reference(workspace, document),
                |_| Ok(()),
            )?;
        }
        InputDocument::Records(document) => {
            stage_payload_bytes(
                workspace,
                input.clone(),
                &document.bytes,
                |workspace| state_records::persist(workspace, document),
                |_| Ok(()),
            )?;
        }
    }
    Ok(())
}
