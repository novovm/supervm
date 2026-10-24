//! Bounded asynchronous observations on the ORIGINAL immutable-tree I/O owner.
//! Reading a grant does not reserve it or authenticate a spend; execution still
//! rechecks and consumes at its exact parent/prefix in the original plan.
use super::super::{Account, Hash, IoTicket};
use crate::native_pipeline::business::uca_spend_state::{
    CapturedUcaSpendStateV1, UcaSpendStateAccessV1,
};
use crate::native_pipeline::pipeline::CandidatePipeline;
use anyhow::{ensure, Context, Result};
use novovm_adapter_api::unified_account::UcaStatus;
use novovm_protocol::native_query::{Answer, Query, QueryError, UcaSpendStateView, UcaState};
use novovm_protocol::uca_spend::UcaSpendIntentV1;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

pub(crate) enum UcaReadResult {
    Missing,
    Present(Box<CapturedUcaSpendStateV1>),
}

/// One bounded read job, usable by admission and query without sharing caches
/// or introducing another authority. Caller binds and rechecks root freshness.
pub(crate) struct UcaStateRead {
    root: Hash,
    access: UcaSpendStateAccessV1,
    keys: Vec<Vec<u8>>,
    values: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    ticket: Option<IoTicket<Option<Vec<u8>>>>,
    completed: bool,
}
impl UcaStateRead {
    pub(crate) fn new(
        root: Hash,
        intent: UcaSpendIntentV1,
        payer: Account,
        recipient: Account,
    ) -> Result<Self> {
        ensure!(root != [0; 32], "UCA read parent missing");
        let access = UcaSpendStateAccessV1::new(intent, payer, recipient)?;
        let keys: Vec<_> = access
            .declarations()
            .iter()
            .map(|d| d.key.clone())
            .collect();
        ensure!(keys.len() <= 80, "UCA read declaration bound");
        Ok(Self {
            root,
            access,
            keys,
            values: BTreeMap::new(),
            ticket: None,
            completed: false,
        })
    }

    pub(crate) fn poll(&mut self, pipeline: &CandidatePipeline) -> Result<Option<UcaReadResult>> {
        ensure!(!self.completed, "UCA read already consumed");
        if self.values.len() < self.keys.len() {
            let key = &self.keys[self.values.len()];
            if self.ticket.is_none() {
                self.ticket = pipeline.try_read_value(self.root, key.clone())?;
            }
            let Some(ticket) = self.ticket.as_mut() else {
                return Ok(None);
            };
            let Some(value) = ticket.try_take()? else {
                return Ok(None);
            };
            // State-tree values are bounded by the original codec. Retain that
            // bound here as well; eight read jobs cannot retain arbitrary data.
            ensure!(
                value.as_ref().is_none_or(|v| v.len() <= 256),
                "UCA read value bound"
            );
            self.values.insert(key.clone(), value);
            self.ticket = None;
            if self.values.len() < self.keys.len() {
                return Ok(None);
            }
        }
        self.completed = true;
        Ok(Some(
            match self.access.read_optional_with(self.root, |key| {
                self.values.get(key).cloned().context("UCA read incomplete")
            })? {
                Some(value) => UcaReadResult::Present(Box::new(value)),
                None => UcaReadResult::Missing,
            },
        ))
    }
}

struct Entry {
    query: Query,
    root: Hash,
    height: u64,
    expires: Instant,
    read: UcaStateRead,
    answer: Option<Answer>,
}

#[derive(Default)]
pub(crate) struct UcaQueries(VecDeque<Entry>);
impl UcaQueries {
    pub(super) fn poll(&mut self, pipeline: &CandidatePipeline, now: Instant) {
        self.0.retain(|e| now < e.expires);
        // As with history reads, never reclaim a released I/O slot in this
        // turn: admission/nonce hydration must get the next opportunity.
        if let Some(index) = self.0.iter().position(|entry| entry.answer.is_none()) {
            let mut entry = self.0.remove(index).unwrap();
            entry.answer = match entry.read.poll(pipeline) {
                Ok(None) => None,
                Err(_) => Some(Answer::Error(QueryError::Unavailable)),
                Ok(Some(UcaReadResult::Missing)) => {
                    Some(Answer::UcaSpendState(UcaState::Missing {
                        state_root: entry.root,
                        height: entry.height,
                    }))
                }
                Ok(Some(UcaReadResult::Present(state))) => Some(Answer::UcaSpendState(
                    UcaState::Present(view(&state, entry.height)),
                )),
            };
            self.0.push_back(entry);
        }
    }

    pub(super) fn read(&mut self, query: Query, root: Hash, height: u64, now: Instant) -> Answer {
        if let Some(index) = self.0.iter().position(|entry| entry.query == query) {
            return if self.0[index].answer.is_some() {
                self.0.remove(index).unwrap().answer.unwrap()
            } else {
                Answer::UcaSpendState(UcaState::Pending)
            };
        }
        if self.0.len() >= 8 {
            return Answer::Error(QueryError::Busy);
        }
        let Query::UcaSpendState {
            intent,
            payer_hint: payer,
            recipient_hint: recipient,
        } = &query
        else {
            return Answer::Error(QueryError::Unavailable);
        };
        let read = (|| {
            UcaStateRead::new(
                root,
                intent.intent(),
                Account::try_from(payer.as_bytes()).map_err(anyhow::Error::msg)?,
                Account::try_from(recipient.as_bytes()).map_err(anyhow::Error::msg)?,
            )
        })();
        match read {
            Err(_) => Answer::Error(QueryError::Unavailable),
            Ok(read) => {
                self.0.push_back(Entry {
                    query,
                    root,
                    height,
                    expires: now + Duration::from_secs(30),
                    read,
                    answer: None,
                });
                Answer::UcaSpendState(UcaState::Pending)
            }
        }
    }
}

fn view(state: &CapturedUcaSpendStateV1, height: u64) -> UcaSpendStateView {
    let grant = state.grant();
    let (payer_balance, recipient_balance) = state.balances();
    UcaSpendStateView {
        state_root: state.parent_root(),
        height,
        account_active: state.payer().account.status == UcaStatus::Active,
        current_spend_epoch: state.payer().current_spend_epoch,
        grant_epoch: grant.authorization_epoch,
        permits_nov_transfer: grant.permits_nov_transfer,
        revoked: grant.revoked,
        not_before_height: grant.not_before_height,
        expires_at_height: grant.expires_at_height,
        max_debit_per_intent: grant.max_debit_per_intent,
        remaining_debit_budget: grant.remaining_debit_budget,
        next_nonce: state.next_nonce(),
        payer_balance,
        recipient_balance,
    }
}

#[cfg(test)]
mod tests;
