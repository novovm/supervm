//! One owner-thread session, eight bounded pure programs, no per-batch setup.

use super::checked_integer_asset::{transfer_asset, INPUTS, OUTPUTS};
#[cfg(test)]
use crate::native_pipeline::business::quoted_transfer::{
    TransferIntent, TransferOutcome, TransferSnapshot,
};
#[cfg(test)]
use anyhow::Context;
use anyhow::{ensure, Result};
use novovm_exec::resident::{
    AoemExecOpenOptions, AoemIntegerInputV1, AoemIntegerPlanNodeV1, AoemIntegerResourceSessionV7,
    AoemIntegerResourceV7, AoemLibrarySelection,
};
#[cfg(test)]
use novovm_exec::resident::{AoemInteger1024V1, AoemIntegerOutcomeKindV1};
use std::path::Path;

/// Lives and is destroyed on the original compute owner. Its native context
/// owns the program/shape table until drop; no authority/store is added here.
pub(crate) struct ResidentTransferResources {
    session: AoemIntegerResourceSessionV7,
    resource: AoemIntegerResourceV7,
    effects: Vec<AoemIntegerResourceV7>,
    capacity: usize,
    poisoned: bool,
}

impl ResidentTransferResources {
    pub(crate) fn new(library: &Path, workers: u32, max_rows: usize) -> Result<Self> {
        ensure!(
            (1..=4096).contains(&max_rows),
            "NOV integer row capacity must be 1..=4096"
        );
        let selection = AoemLibrarySelection::new(
            library,
            AoemExecOpenOptions {
                ingress_workers: Some(workers),
            },
        )?;
        let facade = selection.open_wire_facade()?;
        let mut session = facade.create_integer_resource_session_v7("novovm/native-transfer")?;
        ensure!(
            session.supports_composition_v1(),
            "NOV integer execution requires AOEM composition v1; update SDK, no fallback"
        );
        let resource = session.open(&transfer_asset(), INPUTS, OUTPUTS, max_rows)?;
        let effects = super::effect_assets::assets()
            .into_iter()
            .zip(super::effect_assets::SHAPES)
            .enumerate()
            .map(|(index, (asset, (inputs, outputs)))| {
                // Program 5 consumes memoized invariant quotations, not one
                // row per transaction. Never pad one division to a full batch.
                session.open(
                    &asset,
                    inputs,
                    outputs,
                    if index == 4 { 1 } else { max_rows },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            session,
            resource,
            effects,
            capacity: max_rows,
            poisoned: false,
        })
    }

    pub(super) fn capacity(&self) -> usize {
        self.capacity
    }

    pub(super) fn poison(&mut self) {
        self.poisoned = true;
    }

    pub(super) fn run_effects(
        &mut self,
        nodes: &[AoemIntegerPlanNodeV1],
        exports: &[AoemIntegerInputV1],
    ) -> Result<Vec<u128>> {
        ensure!(!self.poisoned, "NOV integer resource poisoned");
        self.poisoned = true;
        let _span = crate::native_pipeline::profile::span(
            crate::native_pipeline::profile::Phase::IntegerWire,
        );
        let programs: Vec<_> = std::iter::once(&self.resource)
            .chain(&self.effects)
            .collect();
        let values = self.session.run_composition_v1(&programs, nodes, exports)?;
        ensure!(
            values.len() == exports.len(),
            "NOV effect export count mismatch"
        );
        let values = values
            .iter()
            .map(|v| v.try_to_u128())
            .collect::<Result<Vec<_>>>()?;
        self.poisoned = false;
        Ok(values)
    }

    #[cfg(test)]
    pub(super) fn run(
        &mut self,
        inputs: &[(TransferIntent, TransferSnapshot, Option<String>)],
    ) -> Result<Vec<TransferOutcome>> {
        ensure!(
            !self.poisoned,
            "NOV integer resource poisoned; explicit owner recovery required"
        );
        ensure!(
            !inputs.is_empty() && inputs.len() <= self.capacity,
            "invalid NOV integer wave size"
        );
        // The same checked program/shape is reused. Padding is a valid zero-fee
        // transfer, not a transaction, nonce effect or business observation.
        let dummy: Vec<_> = [0, 0, 0, 0, 0, 0, 0, 1, 0, 0]
            .into_iter()
            .map(AoemInteger1024V1::from_u128)
            .collect();
        let mut rows = Vec::with_capacity(self.capacity);
        for (intent, snapshot, rejection) in inputs {
            rows.push(
                [
                    snapshot.payer_balance,
                    snapshot.recipient_balance,
                    intent.amount,
                    intent.approved_fee,
                    intent.fee_cap,
                    u128::from(intent.nonce),
                    u128::from(snapshot.next_nonce),
                    u128::from(!intent.nonce_identity.is_empty()),
                    u128::from(intent.from == intent.to),
                    u128::from(rejection.is_some()),
                ]
                .into_iter()
                .map(AoemInteger1024V1::from_u128)
                .collect(),
            );
        }
        rows.resize(self.capacity, dummy);
        self.poisoned = true;
        let batch = {
            let _span = crate::native_pipeline::profile::span(
                crate::native_pipeline::profile::Phase::IntegerWire,
            );
            if crate::native_pipeline::profile::enabled() {
                let (batch, profile) = self.session.run_profiled(&self.resource, &rows)?;
                crate::native_pipeline::profile::record_integer(
                    &profile,
                    inputs.len(),
                    self.capacity,
                );
                batch
            } else {
                self.session.run_binary_v1(&self.resource, &rows)?
            }
        };
        ensure!(
            batch.rows.len() == self.capacity,
            "NOV integer result row count mismatch"
        );
        let mut outcomes = Vec::with_capacity(inputs.len());
        for (index, row) in batch.rows.iter().enumerate() {
            ensure!(
                row.outcome == AoemIntegerOutcomeKindV1::Value
                    && row.fault == 0
                    && row.component.is_none()
                    && row.failed_instruction.is_none(),
                "NOV checked integer row {index} rejected: {row:?}"
            );
            let values: [u128; OUTPUTS] = row
                .values
                .iter()
                .map(|value| value.try_to_u128())
                .collect::<Result<Vec<_>>>()?
                .try_into()
                .ok()
                .context("NOV checked integer output shape mismatch")?;
            if let Some((intent, snapshot, rejection)) = inputs.get(index) {
                outcomes.push(TransferOutcome::from_checked_integer_values(
                    intent,
                    snapshot,
                    rejection.as_deref(),
                    values,
                )?);
            } else {
                ensure!(
                    values == [0, 0, 1, 0, 0],
                    "NOV integer padding result mismatch"
                );
            }
        }
        self.poisoned = false;
        Ok(outcomes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::business::quoted_transfer::compute_outcome;

    #[test]
    #[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY and Vulkan; current business operator parity, not TPS"]
    fn resident_integer_rows_match_current_quoted_transfer_full_u128() -> Result<()> {
        let library = std::path::PathBuf::from(
            std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
                .context("explicit trusted NOVOVM_AOEM_TEST_LIBRARY required")?,
        );
        let mut resources = ResidentTransferResources::new(&library, 4, 7)?;
        let edges = [
            0,
            1,
            9,
            1 << 64,
            i128::MAX as u128,
            1 << 127,
            u128::MAX - 1,
            u128::MAX,
        ];
        let mut inputs = Vec::new();
        for (i, payer) in edges.into_iter().enumerate() {
            for (j, amount) in edges.into_iter().enumerate() {
                for own in [false, true] {
                    let fee = edges[(i * 3 + j) % edges.len()];
                    inputs.push((
                        TransferIntent {
                            tx_hash: [19; 32],
                            from: [1; 20].into(),
                            to: [if own { 1 } else { 2 }; 20].into(),
                            nonce_identity: "authenticated-signer".into(),
                            nonce: u64::MAX - 1,
                            amount,
                            approved_fee: fee,
                            fee_cap: if (i + j) % 3 == 0 { fee / 2 } else { fee },
                        },
                        TransferSnapshot {
                            payer_balance: payer,
                            recipient_balance: if own {
                                payer
                            } else {
                                edges[(i + j + 3) % edges.len()]
                            },
                            next_nonce: u64::MAX - 1,
                        },
                        None,
                    ));
                }
            }
        }
        let mut rejected = inputs[0].clone();
        rejected.2 = Some("test.existing_quote_rejection".to_owned());
        inputs.push(rejected);
        for chunk in inputs.chunks(7) {
            let expected = chunk
                .iter()
                .map(|(intent, state, rejection)| {
                    compute_outcome(intent, state, rejection.as_deref())
                        .map_err(anyhow::Error::from)
                })
                .collect::<Result<Vec<_>>>()?;
            assert_eq!(resources.run(chunk)?, expected);
        }
        // Domain rejection aborts the batch and fail-closes this product owner;
        // it is not an economic failure receipt or a CPU fallback opportunity.
        let mut invalid = inputs[0].clone();
        invalid.1.next_nonce -= 1;
        assert!(resources.run(&[invalid]).is_err());
        assert!(resources
            .run(&inputs[..1])
            .unwrap_err()
            .to_string()
            .contains("poisoned"));
        Ok(())
    }
}
