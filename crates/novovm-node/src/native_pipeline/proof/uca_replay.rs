//! Public execution replay over the original AOEM signature and NOV effect owners.
//! A replay artifact carries the certified header and exact signed inputs/parent
//! frontier. The consumer independently verifies consensus and executes the input.
use super::{
    capture_budget, plan_budget, wire, BatchContext, DirectNovFeePolicy, NovTransferPlan,
    MAX_INPUT_BYTES, MAX_TRANSACTIONS,
};
use crate::native_pipeline::{
    business::nov_transfer_batch::NovTransferBody,
    consensus::statement::finalized_header::{self, VerifiedHeader},
    persistence::{PacketBudget, PreparedCandidate},
};
use anyhow::{ensure, Context, Result};
use novovm_exec::{resident::AoemLibrarySelection, uca_spend::AssetSpendVerifierV3};
use novovm_protocol::uca_spend::SignedAssetSpendV3;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path, time::Duration};

const MAGIC: &[u8; 8] = b"NVUCAR01";
const MAX_BUNDLE_BYTES: usize = MAGIC.len() + 4 + finalized_header::MAX_BYTES + MAX_INPUT_BYTES;

pub(crate) struct ReplayBundle<'a> {
    certified_header: &'a [u8],
    input: wire::Input<'a>,
}

pub(crate) fn read_bundle(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((MAX_BUNDLE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_BUNDLE_BYTES,
        "UCA replay archive exceeds its size bound"
    );
    Ok(bytes)
}

impl<'a> ReplayBundle<'a> {
    pub(crate) fn decode(bytes: &'a [u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_BUNDLE_BYTES,
            "UCA replay archive exceeds its size bound"
        );
        let bytes = bytes
            .strip_prefix(MAGIC)
            .context("unrecognized UCA replay archive")?;
        let (header_len, bytes) = bytes
            .split_first_chunk::<4>()
            .context("missing UCA header size")?;
        let header_len = u32::from_be_bytes(*header_len) as usize;
        ensure!(
            (1..=finalized_header::MAX_BYTES).contains(&header_len),
            "UCA certified header size bound"
        );
        let (certified_header, input) = bytes
            .split_at_checked(header_len)
            .context("truncated UCA certified header")?;
        Ok(Self {
            certified_header,
            input: wire::decode_uca_replay(input)?,
        })
    }

    pub(crate) fn height(&self) -> u64 {
        self.input.context.height
    }

    pub(crate) fn header(&self) -> &'a [u8] {
        self.certified_header
    }

    pub(crate) fn verify(
        self,
        header: &VerifiedHeader,
        library: &Path,
        library_sha256: &str,
    ) -> Result<()> {
        ensure!(
            &self.input.context == header.execution_context(),
            "UCA replay input does not match the certified execution context"
        );
        let library = library.canonicalize()?;
        verify_library_digest(&library, library_sha256)?;
        let selection = AoemLibrarySelection::new(
            &library,
            novovm_exec::AoemExecOpenOptions {
                ingress_workers: Some(1),
            },
        )?;
        let verifier = selection.open_wire_facade()?.asset_spend_verifier_v3()?;
        let mut compute = selection.open_compute()?;
        verify_library_digest(&library, library_sha256)?;
        let plan = prepare_plan(
            &verifier,
            self.input.context,
            &self.input.raw,
            self.input.policy,
        )?;
        let result = plan
            .capture_witness(self.input.witness, capture_budget())?
            .execute(&mut compute, Duration::from_secs(30))?;
        let packet = PreparedCandidate::from_executed(result, PacketBudget::default())?;
        header.check_replayed_candidate(&packet)
    }
}

pub(crate) fn prepare_plan(
    verifier: &AssetSpendVerifierV3,
    context: BatchContext,
    rows: &[Vec<u8>],
    policy: DirectNovFeePolicy,
) -> Result<NovTransferPlan> {
    ensure!(
        (1..=MAX_TRANSACTIONS).contains(&rows.len()),
        "UCA replay transaction count"
    );
    let signed = rows
        .iter()
        .map(|row| {
            let signed = SignedAssetSpendV3::decode(row)?;
            ensure!(
                signed.intent.chain_id == context.chain_id
                    && signed.intent.genesis == context.genesis_config_commitment,
                "UCA replay transaction domain differs from the certified block"
            );
            Ok(signed)
        })
        .collect::<Result<Vec<_>>>()?;
    let checked = verifier
        .verify_batch(signed)?
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        checked.len() == rows.len(),
        "UCA replay signature result count"
    );
    NovTransferBody::prepare_asset_spend(checked, policy, plan_budget())?.bind(context)
}

fn verify_library_digest(path: &Path, expected: &str) -> Result<()> {
    let mut source = File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        let count = source.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
    }
    ensure!(
        format!("{:x}", digest.finalize()) == expected,
        "UCA replay AOEM library differs from the installed release pin"
    );
    Ok(())
}

#[cfg(test)]
pub(crate) fn encode(header: &[u8], input: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        (1..=finalized_header::MAX_BYTES).contains(&header.len()) && input.len() <= MAX_INPUT_BYTES,
        "UCA replay bundle component exceeds bounds"
    );
    let mut bytes = Vec::with_capacity(MAGIC.len() + 4 + header.len() + input.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&u32::try_from(header.len())?.to_be_bytes());
    bytes.extend_from_slice(header);
    bytes.extend_from_slice(input);
    Ok(bytes)
}
