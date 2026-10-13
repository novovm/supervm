//! Bounded, caller-located historical receipts on the existing durable archive.
//! This is a derived query, not another transaction index or authority. A height
//! is a locator only: the local validated prefix and archive evidence decide
//! whether a receipt is final. Hash-only lookup keeps its existing cache scope.

use super::{hex, hex_param, receipt_json, Hash};
use crate::native_pipeline::consensus::statement::ParentPoint;
use crate::native_pipeline::consensus::wire::{Context as ConsensusContext, ValidatorSet};
use crate::native_pipeline::consensus::{ArchiveBlock, ArchiveRead};
use crate::native_pipeline::pipeline::CandidatePipeline;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Arc;

const MAX_QUERIES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Locator {
    hash: Hash,
    height: u64,
}

struct Query {
    locator: Locator,
    read: Option<ArchiveRead>,
    result: Option<std::result::Result<Value, String>>,
}

#[derive(Default)]
pub(super) struct ReceiptHistory {
    queries: VecDeque<Query>,
}

/// Backward compatible positional form: [hash] or [hash, positive_u64_height].
/// Do not coerce strings/floats, silently ignore extra fields, or infer a block
/// from untrusted pending input. A hash by itself never scans all history.
pub(super) fn parse_lookup(request: &Value) -> Result<(Hash, Option<u64>)> {
    let params = request["params"]
        .as_array()
        .context("params array required")?;
    ensure!(
        (1..=2).contains(&params.len()),
        "transaction hash and optional block height required"
    );
    let hash = hex_param(&params[0], 32)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("32-byte hash required"))?;
    let height = params
        .get(1)
        .map(|value| {
            value
                .as_u64()
                .filter(|height| *height != 0)
                .context("block height must be a positive u64 integer")
        })
        .transpose()?;
    Ok((hash, height))
}

impl ReceiptHistory {
    pub(super) fn len(&self) -> usize {
        self.queries.len()
    }

    pub(super) fn lookup(
        &mut self,
        hash: Hash,
        height: u64,
        ceiling: ParentPoint,
        context: ConsensusContext,
        validators: Arc<ValidatorSet>,
    ) -> Result<Value> {
        ensure!(
            height != 0 && height <= ceiling.height,
            "historical receipt height is outside locally validated decided prefix"
        );
        let locator = Locator { hash, height };
        if let Some(result) = self.existing(locator) {
            return result;
        }
        // Construct/validate before evicting any completed result. Untrusted
        // locators or invalid authority context must not mutate the queue.
        let read = ArchiveRead::new(height, ceiling, context, validators)?;
        self.reserve_slot()?;
        self.queries.push_back(Query {
            locator,
            read: Some(read),
            result: None,
        });
        Ok(pending(locator))
    }

    fn existing(&mut self, locator: Locator) -> Option<Result<Value>> {
        let position = self.queries.iter().position(|q| q.locator == locator)?;
        // Preserve completed results until bounded eviction so a batch poll or
        // second client does not restart an earlier-finished read. Nothing here
        // authorizes execution or replaces the durable archive/head.
        Some(match &self.queries[position].result {
            None => Ok(pending(locator)),
            Some(Ok(result)) => Ok(result.clone()),
            Some(Err(error)) => Err(anyhow::Error::msg(error.clone())),
        })
    }

    fn reserve_slot(&mut self) -> Result<()> {
        if self.queries.len() == MAX_QUERIES {
            if let Some(position) = self.queries.iter().position(|q| q.result.is_some()) {
                self.queries.remove(position);
            }
        }
        ensure!(
            self.queries.len() < MAX_QUERIES,
            "historical receipt query backpressure; retry"
        );
        Ok(())
    }

    /// At most ONE archive poll across all historical queries per control turn.
    /// The shared metadata budget may be one slot, held until a finished ticket
    /// is consumed. Once we consume that ticket, a later historical query must
    /// not reserve the freed slot before the next consensus/projection turn.
    /// Rotate complete queries as well, without a cursor invalidated by eviction.
    fn poll_one(&mut self, poll: impl FnOnce(&mut Query)) {
        let Some(position) = self.queries.iter().position(|query| query.read.is_some()) else {
            return;
        };
        let mut query = self.queries.remove(position).expect("pending query");
        poll(&mut query);
        self.queries.push_back(query);
    }

    /// No blocking I/O or full history traversal. Each completed/error query
    /// releases its archive and candidate before retaining its small RPC result.
    /// Errors belong to the query and never stop consensus or imply absence.
    pub(super) fn poll(&mut self, pipeline: &CandidatePipeline) {
        self.poll_one(|query| {
            let read = query.read.as_mut().expect("selected pending query");
            let result = match read.poll(pipeline) {
                Ok(None) => return,
                Ok(Some(block)) => completed_receipt(query.locator, block),
                Err(error) => Err(error),
            };
            query.read = None;
            query.result = Some(
                result
                    .map_err(|error| format!("historical receipt archive unavailable: {error:#}")),
            );
        });
    }
}

fn pending(locator: Locator) -> Value {
    json!({"tx_hash":hex(&locator.hash),"block_height":locator.height,
        "state":"query_pending","lookup_complete":false,"query_scope":"block",
        "executed":false,"finalized":false})
}

fn completed_receipt(locator: Locator, block: Option<ArchiveBlock>) -> Result<Value> {
    // Every requested height is within an already validated local prefix. A
    // missing archive block is broken durable evidence, not an absent tx.
    let block = block.context("decided block missing from locally validated archive")?;
    ensure!(
        block.point().height == locator.height,
        "historical receipt archive returned another height"
    );
    for index in 0..block.stored().receipt_bytes().len() {
        let receipt = block.stored().receipt_view(index)?;
        if receipt.tx_hash == locator.hash {
            let mut result = receipt_json(receipt, block.point().height, block.point().block_hash);
            result["lookup_complete"] = json!(true);
            result["query_scope"] = json!("block");
            return Ok(result);
        }
    }
    Ok(json!({"tx_hash":hex(&locator.hash),"state":"not_in_block",
        "block_height":locator.height,"block_hash":hex(&block.point().block_hash),
        "lookup_complete":true,"query_scope":"block",
        "executed":false,"finalized":false}))
}

#[cfg(test)]
mod tests;
