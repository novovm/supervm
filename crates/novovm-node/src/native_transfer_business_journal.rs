//! A compact effect for the existing bounded fee journal, not a new codec.
//! NOV fee settlement appends at most one entry. Prove that shape and the exact
//! retained prefix before returning a delta; never infer it from fee success.
use super::{NovTreasurySettlementJournalEntryV1, NOV_TREASURY_SETTLEMENT_JOURNAL_MAX_ENTRIES_V1};
use anyhow::{bail, Context, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct FeeJournalDeltaV1 {
    before_len: usize,
    before_sequence: u64,
    drop_front: usize,
    appended: Vec<NovTreasurySettlementJournalEntryV1>,
    after_sequence: u64,
}

impl FeeJournalDeltaV1 {
    pub(super) fn between(
        before: &[NovTreasurySettlementJournalEntryV1],
        before_sequence: u64,
        after: &[NovTreasurySettlementJournalEntryV1],
        after_sequence: u64,
    ) -> Result<Self> {
        let limit = NOV_TREASURY_SETTLEMENT_JOURNAL_MAX_ENTRIES_V1;
        if before.len() > limit || after.len() > limit {
            bail!("AOEM fee journal exceeds fixed bound");
        }
        let appended_count = after_sequence
            .checked_sub(before_sequence)
            .context("AOEM fee journal sequence regressed")?;
        if appended_count == 0 {
            if before != after {
                bail!("AOEM fee journal changed without advancing its sequence");
            }
            return Ok(Self {
                before_len: before.len(),
                before_sequence,
                drop_front: 0,
                appended: Vec::new(),
                after_sequence,
            });
        }
        if appended_count != 1 {
            bail!("NOV Transfer fee settlement appended more than one journal entry");
        }
        let drop_front = (before.len() + 1).saturating_sub(limit);
        let retained = before.len() - drop_front;
        if after.len() != retained + 1 || before[drop_front..] != after[..retained] {
            bail!("AOEM fee journal modified its retained prefix");
        }
        let entry = after.last().context("missing AOEM fee journal append")?;
        if entry.seq != after_sequence {
            bail!("AOEM fee journal append sequence differs from its counter");
        }
        Ok(Self {
            before_len: before.len(),
            before_sequence,
            drop_front,
            appended: vec![entry.clone()],
            after_sequence,
        })
    }

    pub(super) fn validate_prefix(
        &self,
        journal: &[NovTreasurySettlementJournalEntryV1],
        sequence: u64,
    ) -> Result<()> {
        let limit = NOV_TREASURY_SETTLEMENT_JOURNAL_MAX_ENTRIES_V1;
        if journal.len() != self.before_len
            || sequence != self.before_sequence
            || self.before_len > limit
            || self.appended.len() > 1
            || self.drop_front > self.before_len
            || self.drop_front != (self.before_len + self.appended.len()).saturating_sub(limit)
            || self.before_sequence.checked_add(self.appended.len() as u64)
                != Some(self.after_sequence)
            || self
                .appended
                .last()
                .is_some_and(|entry| entry.seq != self.after_sequence)
        {
            bail!("AOEM fee journal effect differs from ordered candidate prefix");
        }
        Ok(())
    }

    pub(super) fn apply(
        &self,
        journal: &mut Vec<NovTreasurySettlementJournalEntryV1>,
        sequence: &mut u64,
    ) -> Result<()> {
        self.validate_prefix(journal, *sequence)?;
        journal.drain(..self.drop_front);
        journal.extend(self.appended.iter().cloned());
        *sequence = self.after_sequence;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(seq: u64) -> NovTreasurySettlementJournalEntryV1 {
        let encoded = serde_json::json!({
            "seq": seq, "unix_ms": 0, "kind": "fee", "tx_hash": format!("tx-{seq}"),
            "source_asset": "NOV", "source_amount": 1, "settled_nov": 1,
            "reserve_bucket_delta_nov": 1, "fee_bucket_delta_nov": 0,
            "risk_buffer_delta_nov": 0, "status": "settled", "reason": null,
        });
        serde_json::from_value(encoded).unwrap()
    }

    #[test]
    fn journal_delta_empty_append_and_rejection_are_exact() {
        for (before, before_seq, after, after_seq) in [
            (vec![], 0, vec![], 0),
            (vec![], 0, vec![entry(1)], 1),
            (vec![entry(1)], 1, vec![entry(1)], 1),
            (vec![entry(1)], 1, vec![entry(1), entry(2)], 2),
        ] {
            let delta = FeeJournalDeltaV1::between(&before, before_seq, &after, after_seq).unwrap();
            assert!(delta.appended.len() <= 1);
            let mut actual = before;
            let mut sequence = before_seq;
            delta.apply(&mut actual, &mut sequence).unwrap();
            assert_eq!(actual, after);
            assert_eq!(sequence, after_seq);
        }
    }

    #[test]
    fn journal_delta_full_window_keeps_prefix_and_only_one_new_entry() {
        let limit = NOV_TREASURY_SETTLEMENT_JOURNAL_MAX_ENTRIES_V1;
        let before: Vec<_> = (1..=limit as u64).map(entry).collect();
        let after: Vec<_> = (2..=limit as u64 + 1).map(entry).collect();
        let delta =
            FeeJournalDeltaV1::between(&before, limit as u64, &after, limit as u64 + 1).unwrap();
        assert_eq!(delta.drop_front, 1);
        assert_eq!(delta.appended, vec![entry(limit as u64 + 1)]);
        let mut actual = before.clone();
        let mut sequence = limit as u64;
        delta.apply(&mut actual, &mut sequence).unwrap();
        assert_eq!(actual, after);
        assert_eq!(sequence, limit as u64 + 1);
        let mut corrupted = after;
        corrupted[20].tx_hash.push_str("-changed");
        assert!(
            FeeJournalDeltaV1::between(&before, limit as u64, &corrupted, limit as u64 + 1)
                .is_err()
        );
    }

    #[test]
    fn journal_delta_rejects_bad_sequence_shape_and_stale_application() {
        let before = vec![entry(u64::MAX)];
        let unchanged = FeeJournalDeltaV1::between(&before, u64::MAX, &before, u64::MAX)
            .expect("a rejected fee leaves an exhausted journal unchanged");
        let mut actual = before.clone();
        let mut sequence = u64::MAX;
        unchanged.apply(&mut actual, &mut sequence).unwrap();
        assert!(FeeJournalDeltaV1::between(&before, u64::MAX, &[entry(0)], 0).is_err());
        assert!(FeeJournalDeltaV1::between(
            &before,
            u64::MAX,
            &[entry(u64::MAX), entry(u64::MAX)],
            u64::MAX
        )
        .is_err());
        assert!(FeeJournalDeltaV1::between(&[], 0, &[entry(1), entry(2)], 2).is_err());
        assert!(FeeJournalDeltaV1::between(&[], 0, &[entry(2)], 1).is_err());
        let delta = FeeJournalDeltaV1::between(&[], 0, &[entry(1)], 1).unwrap();
        let mut stale = vec![entry(5)];
        let mut stale_seq = 5;
        assert!(delta.apply(&mut stale, &mut stale_seq).is_err());
        assert_eq!(stale, vec![entry(5)]);
        assert_eq!(stale_seq, 5);
    }

    #[test]
    fn business_digest_stream_matches_previous_json_bytes() {
        let entries = vec![entry(1), entry(2)];
        let expected = super::super::sha256_bytes_v1(&[
            b"novovm-native-transfer-business-effects-v1\0",
            &serde_json::to_vec(&entries).unwrap(),
        ]);
        assert_eq!(
            super::super::business_effects_digest(&entries).unwrap(),
            expected
        );
    }
}
