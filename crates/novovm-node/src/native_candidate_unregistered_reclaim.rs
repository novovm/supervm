//! Reclaim only completed, private outputs absent from the validated ledger.
//! The temporary journal is not the permanent finalized-retirement tombstone:
//! a legitimately proposed identical plan may be captured again after cleanup.

use super::*;
use execution::{
    is_complete, load_block_artifact_inner_v1, output_chunk_key, publication_target_fields,
    OutputDescriptor,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnregisteredReclaimCheckpointV1 {
    IntentPersisted,
    PartialReclaim,
    SlotReleased,
}

struct Reclaim {
    slot: usize,
    genesis: [u8; 32],
    height: u64,
    block_hash: [u8; 32],
    input: Descriptor,
    output: OutputDescriptor,
}

impl Reclaim {
    fn encode(&self, scope: &[u8; 32]) -> Vec<u8> {
        let mut raw = b"NCU1".to_vec();
        raw.extend_from_slice(&(self.slot as u32).to_be_bytes());
        raw.extend_from_slice(&self.genesis);
        raw.extend_from_slice(&self.height.to_be_bytes());
        raw.extend_from_slice(&self.block_hash);
        raw.extend_from_slice(&self.input.encode());
        raw.extend_from_slice(&self.output.encode());
        // Corruption detection, not authentication against an actor capable
        // of rewriting the database and recomputing this public checksum.
        let digest = sha256_bytes_v1(&[b"novovm-unregistered-reclaim-v1\0", scope, &raw]);
        raw.extend_from_slice(&digest);
        raw
    }

    fn decode(raw: &[u8], scope: &[u8; 32]) -> Result<Self> {
        const INPUT: usize = 4 + 4 + 32 + 8 + 32;
        const CONTENT: usize = INPUT + DESCRIPTOR_BYTES + 76;
        if raw.len() != CONTENT + 32 || &raw[..4] != b"NCU1" {
            bail!("invalid unregistered reclaim journal codec");
        }
        if raw[CONTENT..]
            != sha256_bytes_v1(&[b"novovm-unregistered-reclaim-v1\0", scope, &raw[..CONTENT]])
        {
            bail!("unregistered reclaim journal checksum mismatch");
        }
        let input = Descriptor::decode(&raw[INPUT..INPUT + DESCRIPTOR_BYTES], scope)?;
        let output = OutputDescriptor::decode(&raw[INPUT + DESCRIPTOR_BYTES..CONTENT], &input)?;
        let result = Self {
            slot: u32::from_be_bytes(raw[4..8].try_into()?) as usize,
            genesis: raw[8..40].try_into()?,
            height: u64::from_be_bytes(raw[40..48].try_into()?),
            block_hash: raw[48..80].try_into()?,
            input,
            output,
        };
        if result.slot >= MAX_WORKSPACES_V1
            || result.genesis == [0; 32]
            || result.height < 2
            || result.block_hash == [0; 32]
        {
            bail!("invalid unregistered reclaim journal bounds");
        }
        Ok(result)
    }

    fn deleted_keys(&self, workspace: &WorkspaceStore) -> Vec<Vec<u8>> {
        let mut keys = Vec::new();
        for index in 0..self.input.len.div_ceil(CHUNK_BYTES) {
            keys.push(workspace.chunk_key(&self.input.id, index));
        }
        for index in 0..self.output.len.div_ceil(CHUNK_BYTES) {
            keys.push(output_chunk_key(workspace, &self.input.id, index));
        }
        for kind in [b'v', b'e', b'r'] {
            keys.push(workspace.key(kind, &self.input.id));
        }
        keys
    }
}

pub(crate) fn reclaim_unregistered_workspaces_v1(
    chain: u64,
    current: [u8; 32],
    genesis: [u8; 32],
    ids: &[[u8; 32]],
    params: &serde_json::Value,
) -> Result<Vec<[u8; 32]>> {
    reclaim_unregistered_with_checkpoint_v1(chain, current, genesis, ids, params, |_| Ok(()))
}

pub(crate) fn reclaim_unregistered_with_checkpoint_v1(
    chain: u64,
    current: [u8; 32],
    genesis: [u8; 32],
    ids: &[[u8; 32]],
    params: &serde_json::Value,
    checkpoint: impl Fn(UnregisteredReclaimCheckpointV1) -> Result<()>,
) -> Result<Vec<[u8; 32]>> {
    if ids.len() > MAX_WORKSPACES_V1 || ids.contains(&[0; 32]) {
        bail!("unregistered reclaim request exceeds workspace bounds");
    }
    let mut workspace = WorkspaceStore::open(chain, params)?;
    let image = live_parent::capture_finalized_parent_view_locked(
        &mut workspace,
        current,
        genesis,
        params,
    )?;
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("unregistered reclaim requires explicit native path")?;
    // Same lock order as publication: workspace -> authority -> ledger. Parent
    // capture released its own authority guard, so recheck the exact target.
    let _authority = acquire_nov_native_execution_store_write_lock_v1(&native_path)?;
    let ledger = nov_native_block_ledger_rocksdb_path_v1(&native_path);
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "unregistered reclaim namespace")?;
    let (binding, commitment, block) = NovNativeBlockLedgerV1::load_fresh_finalized_execution_v1(
        &ledger,
        genesis,
        namespace,
        image.block().header.height,
    )?;
    let catalog = workspace.catalog()?;
    let current_input = catalog
        .iter()
        .find(|(_, input)| input.id == current)
        .context("unregistered reclaim current input missing")?
        .1
        .clone();
    if binding.workspace_id != current
        || binding.plan_commitment != current_input.plan
        || binding.output_digest != image.output_digest()
        || &block != image.block()
    {
        bail!("unregistered reclaim authority changed after verification");
    }
    let target = publication_target_fields(
        if block.header.height == 1 {
            b"NVP1"
        } else {
            b"NVP2"
        },
        namespace,
        genesis,
        commitment,
        current,
        image.output_digest(),
        &block,
    );
    let head_key = native_aoem_owned_state_head_key_v1(chain, &workspace.namespace);
    if workspace.graph.get(&head_key)?.as_deref() != Some(target.as_slice()) {
        bail!("unregistered reclaim requires the exact current authority");
    }

    // Complete artifact validation may read the ledger. Finish it BEFORE the
    // ledger callback below acquires its non-reentrant registration write lock.
    let mut prepared = Vec::new();
    let mut published_ids = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(*id) || *id == current {
            continue;
        }
        let journal = workspace.graph.get(&workspace.key(b'q', id))?;
        let Some((slot, input)) = catalog.iter().find(|(_, input)| input.id == *id) else {
            if journal.is_some() {
                bail!("unregistered reclaim journal has no catalog slot");
            }
            continue; // Already reclaimed; no permanent marker or new writes.
        };
        if workspace.graph.get(&workspace.key(b'h', id))?.is_some() {
            if journal.is_some() {
                bail!("unregistered reclaim journal conflicts with publication evidence");
            }
            // Normal startup scans include older registered finalized parents.
            // Their publication evidence protects them; an unregistered h is
            // not silently ignored and is checked under the ledger lock below.
            published_ids.push(*id);
            continue;
        }
        if workspace.graph.get(&workspace.key(b'g', id))?.is_some()
            || workspace.graph.get(&workspace.key(b'a', id))?.is_some()
        {
            continue; // Existing retirement/abort policies remain unchanged.
        }
        let reclaim = if let Some(raw) = &journal {
            // A crash may already have removed chunks. The durable journal's
            // exact input/output bounds, not guessed absent bytes, drive replay.
            Reclaim::decode(raw, &workspace.scope)?
        } else {
            if workspace.status(*slot, input)? != WorkspaceStatusV1::Ready {
                continue;
            }
            let Some(raw) = workspace.graph.get(&workspace.key(b'v', id))? else {
                continue;
            };
            let output = OutputDescriptor::decode(&raw, input)?;
            if !is_complete(&workspace, input, &output)? {
                continue;
            }
            // This cleanup applies only to successors of a verified finalized
            // parent, never fresh-genesis/bootstrap or legacy transaction-parent
            // workspaces. Existing retirement handles their separate lifecycle.
            let successor = match workspace.read_input(input)? {
                VerifiedInput::Light(_) => true,
                VerifiedInput::Cold(payload) => payload.finalized_parent.is_some(),
            };
            if !successor {
                continue;
            }
            let artifact = load_block_artifact_inner_v1(&workspace, *id, params)?
                .context("unregistered reclaim completed artifact missing")?;
            if artifact.workspace_id != *id
                || artifact.plan_commitment != input.plan
                || artifact.output_digest != output.digest
                || artifact.block().header.height < 2
                || !artifact.fresh_genesis_identity().is_some_and(|identity| {
                    identity.chain_id() == chain && identity.config_commitment() == genesis
                })
            {
                bail!("unregistered reclaim artifact binding mismatch");
            }
            Reclaim {
                slot: *slot,
                genesis,
                height: artifact.block().header.height,
                block_hash: artifact.block().header.block_hash,
                input: input.clone(),
                output,
            }
        };
        if reclaim.slot != *slot || reclaim.input != *input || reclaim.genesis != genesis {
            bail!("unregistered reclaim journal binding mismatch");
        }
        prepared.push((reclaim, journal.is_some()));
    }

    NovNativeBlockLedgerV1::with_unregistered_fresh_workspaces_v1(
        &ledger,
        genesis,
        namespace,
        current,
        ids,
        |unregistered| {
            if published_ids.iter().any(|id| unregistered.contains(id)) {
                bail!("unregistered reclaim found publication evidence without registration");
            }
            let mut reclaimed = Vec::new();
            for (reclaim, resumed) in prepared {
                let input = &reclaim.input;
                if !unregistered.contains(&input.id) {
                    if resumed {
                        bail!("unregistered reclaim journal conflicts with registered candidate");
                    }
                    continue;
                }
                let journal_key = workspace.key(b'q', &input.id);
                let marker = AoemAtomicGraphWriteV1::Put {
                    key: journal_key.clone(),
                    value: reclaim.encode(&workspace.scope),
                };
                if !resumed {
                    workspace.commit(b'Q', input, vec![marker.clone()], marker.clone())?;
                }
                if workspace.graph.get(&journal_key)? != Some(reclaim.encode(&workspace.scope))
                    || workspace.graph.get(&workspace.slot_key(reclaim.slot))?
                        != Some(input.encode())
                    || workspace.graph.get(&head_key)?.as_deref() != Some(target.as_slice())
                {
                    bail!("unregistered reclaim intent readback mismatch");
                }
                checkpoint(UnregisteredReclaimCheckpointV1::IntentPersisted)?;
                workspace.commit(
                    b'J',
                    input,
                    vec![AoemAtomicGraphWriteV1::Delete {
                        key: workspace.chunk_key(&input.id, 0),
                    }],
                    marker.clone(),
                )?;
                checkpoint(UnregisteredReclaimCheckpointV1::PartialReclaim)?;
                // Delete only this candidate's private c/o chunks and v/e/r
                // metadata. Shared content-addressed records/trees, h/g/a and
                // all ledger/QC/nonce/pool evidence are deliberately untouched.
                let keys = reclaim.deleted_keys(&workspace);
                let writes = keys
                    .iter()
                    .map(|key| AoemAtomicGraphWriteV1::Delete { key: key.clone() })
                    .collect();
                workspace.commit(b'K', input, writes, marker)?;
                for key in &keys {
                    if workspace.graph.get(key)?.is_some() {
                        bail!("unregistered reclaim private deletion readback mismatch");
                    }
                }
                if workspace.graph.get(&journal_key)? != Some(reclaim.encode(&workspace.scope))
                    || workspace.graph.get(&workspace.slot_key(reclaim.slot))?
                        != Some(input.encode())
                    || workspace.graph.get(&head_key)?.as_deref() != Some(target.as_slice())
                {
                    bail!("unregistered reclaim journal or authority readback mismatch");
                }
                // One atomic write set removes BOTH slot and temporary journal.
                // The completion merely repeats the journal delete. No crash
                // can free the slot while leaving a discoverability-free q.
                workspace.commit(
                    b'Z',
                    input,
                    vec![
                        AoemAtomicGraphWriteV1::Delete {
                            key: workspace.slot_key(reclaim.slot),
                        },
                        AoemAtomicGraphWriteV1::Delete {
                            key: journal_key.clone(),
                        },
                    ],
                    AoemAtomicGraphWriteV1::Delete {
                        key: journal_key.clone(),
                    },
                )?;
                checkpoint(UnregisteredReclaimCheckpointV1::SlotReleased)?;
                for key in &keys {
                    if workspace.graph.get(key)?.is_some() {
                        bail!("unregistered reclaim final deletion readback mismatch");
                    }
                }
                if workspace
                    .graph
                    .get(&workspace.slot_key(reclaim.slot))?
                    .is_some()
                    || workspace.graph.get(&journal_key)?.is_some()
                    || workspace.graph.get(&head_key)?.as_deref() != Some(target.as_slice())
                {
                    bail!("unregistered reclaim completion or authority readback mismatch");
                }
                reclaimed.push(input.id);
            }
            Ok(reclaimed)
        },
    )
}

#[cfg(test)]
#[path = "native_candidate_unregistered_reclaim_tests.rs"]
mod tests;
#[cfg(test)]
pub(crate) use tests::exercise_unregistered_reclaim_for_test_v1;

#[cfg(test)]
mod codec_tests {
    use super::*;

    #[test]
    fn unregistered_reclaim_journal_codec_binds_fields_and_scope() {
        let scope = [0x31; 32];
        let plan = [0x32; 32];
        let input = Descriptor {
            version: DescriptorVersion::Ncw2,
            id: workspace_id(&scope, &plan),
            plan,
            payload: [0x33; 32],
            parent_block: [0x34; 32],
            parent_state: [0x35; 32],
            parent_snapshot: [0x36; 32],
            len: 512,
        };
        let mut output_raw = b"NCE1".to_vec();
        output_raw.extend_from_slice(&512u64.to_be_bytes());
        output_raw.extend_from_slice(&[0x37; 32]);
        output_raw.extend_from_slice(&input.payload);
        let output = OutputDescriptor::decode(&output_raw, &input).unwrap();
        let reclaim = Reclaim {
            slot: 1,
            genesis: [0x38; 32],
            height: 2,
            block_hash: [0x39; 32],
            input,
            output,
        };
        let encoded = reclaim.encode(&scope);
        assert_eq!(encoded.len(), 392);
        assert_eq!(
            Reclaim::decode(&encoded, &scope).unwrap().encode(&scope),
            encoded
        );
        // Each byte flip still has a structurally legitimate value: a new slot,
        // height, block hash, output length or digest must not silently drive
        // deletion after the corresponding chunks have already disappeared.
        for offset in [
            7,
            47,
            48,
            80 + DESCRIPTOR_BYTES + 11,
            80 + DESCRIPTOR_BYTES + 12,
        ] {
            let mut changed = encoded.clone();
            changed[offset] ^= 1;
            let error = Reclaim::decode(&changed, &scope).err().unwrap();
            assert!(error.to_string().contains("checksum mismatch"), "{error:#}");
        }
        for length in 0..encoded.len() {
            assert!(Reclaim::decode(&encoded[..length], &scope).is_err());
        }
        let mut extended = encoded.clone();
        extended.push(0);
        assert!(Reclaim::decode(&extended, &scope).is_err());
        let other_scope = [0x41; 32];
        assert!(Reclaim::decode(&encoded, &other_scope).is_err());
        // Even rewriting the checksum for another scope cannot change the
        // descriptor's independently derived workspace identity.
        assert!(Reclaim::decode(&reclaim.encode(&other_scope), &other_scope).is_err());
    }
}
