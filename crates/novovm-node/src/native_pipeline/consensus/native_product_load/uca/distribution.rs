//! Account dependency shapes for the original native load fixture, not node limits.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Recipients {
    Shared,
    Dispersed,
    Mixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WorkloadShape {
    pub payers: usize,
    recipients: Recipients,
}

impl WorkloadShape {
    pub(super) fn hotspot() -> Self {
        Self {
            payers: SENDERS,
            recipients: Recipients::Shared,
        }
    }

    pub(super) fn from_environment(count: usize) -> Result<Self> {
        let payers = env_count("NOVOVM_NATIVE_UCA_PAYERS", SENDERS, count)?;
        let recipients = match std::env::var("NOVOVM_NATIVE_UCA_RECIPIENT_DISTRIBUTION")
            .unwrap_or_else(|_| "shared".into())
            .as_str()
        {
            "shared" => Recipients::Shared,
            "dispersed" => Recipients::Dispersed,
            "mixed" => Recipients::Mixed,
            _ => bail!("recipient distribution must be shared, dispersed or mixed"),
        };
        ensure!(
            count.is_multiple_of(payers),
            "load requires complete payer nonce rounds"
        );
        Ok(Self { payers, recipients })
    }

    pub(super) fn recipients(self) -> usize {
        match self.recipients {
            Recipients::Shared => 1,
            Recipients::Dispersed => self.payers,
            Recipients::Mixed => self.payers + 1,
        }
    }

    pub(super) fn accounts(self) -> usize {
        self.payers + self.recipients()
    }

    pub(super) fn recipient(self, row: usize) -> usize {
        self.payers
            + match self.recipients {
                Recipients::Shared => 0,
                Recipients::Dispersed => row % self.payers,
                Recipients::Mixed if row.is_multiple_of(4) => 0,
                Recipients::Mixed => 1 + row % self.payers,
            }
    }

    pub(super) fn credits(self, count: usize) -> Vec<u128> {
        let mut credits = vec![0; self.recipients()];
        for index in 0..count {
            credits[self.recipient(index) - self.payers] += 1;
        }
        credits
    }

    pub(super) fn online_accounts(self) -> Vec<usize> {
        if self.accounts() <= 64 {
            return (0..self.accounts()).collect();
        }
        (0..16)
            .map(|sample| sample * (self.accounts() - 1) / 15)
            .chain([self.payers])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub(super) fn description(self, count: usize) -> Value {
        json!({
            "payer_accounts":self.payers,"transactions_per_payer":count / self.payers,
            "recipient_accounts":self.recipients(),"total_asset_accounts":self.accounts(),
            "recipient_distribution":match self.recipients {
                Recipients::Shared => "one separate shared recipient",
                Recipients::Dispersed => "one separate recipient per payer; row modulo payer count",
                Recipients::Mixed => "every fourth row to shared recipient, others to payer-specific separate recipient",
            },
            "independent_management_and_spend_keys_per_account":true,
            "identity_scope":"one synthetic UCA association; independent asset accounts and keys, not an acceptance of independently registered UCA identities",
        })
    }
}

#[test]
fn four_account_distributions_keep_independent_nonces_and_conserved_credits() {
    for (payers, recipients) in [
        (16, Recipients::Shared),
        (1024, Recipients::Shared),
        (8192, Recipients::Mixed),
        (65_536, Recipients::Dispersed),
    ] {
        let shape = WorkloadShape { payers, recipients };
        let mut nonces = vec![0u64; payers];
        let mut seen = BTreeSet::new();
        for index in 0..65_536 {
            let payer = index % payers;
            assert_eq!(nonces[payer], (index / payers) as u64);
            nonces[payer] += 1;
            let recipient = shape.recipient(index);
            assert!((payers..shape.accounts()).contains(&recipient));
            assert_ne!(account_bytes(payer), account_bytes(recipient));
            seen.insert(recipient);
        }
        assert!(nonces
            .iter()
            .all(|nonce| *nonce == (65_536 / payers) as u64));
        let credits = shape.credits(65_536);
        assert_eq!(credits.iter().sum::<u128>(), 65_536);
        if recipients == Recipients::Dispersed {
            assert_eq!(seen.len(), payers);
            assert!(credits.iter().all(|credit| *credit == 1));
        }
        let addresses = (0..shape.accounts())
            .map(account_bytes)
            .collect::<BTreeSet<_>>();
        assert_eq!(addresses.len(), shape.accounts());
    }
}
