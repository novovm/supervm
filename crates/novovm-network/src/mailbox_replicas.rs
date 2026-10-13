//! Local placement policy, not a proof of physical independence. The owner must
//! establish failure domains; unsolicited peer claims must not populate them.
//! Unknown domains can provide opportunities but never satisfy durable redundancy.
use anyhow::{ensure, Result};
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub struct FailureDomainsV1 {
    pub operator: [u8; 32],
    pub device: [u8; 32],
    pub network: [u8; 32],
    pub power: [u8; 32],
}
impl FailureDomainsV1 {
    fn known(self) -> bool {
        [self.operator, self.device, self.network, self.power]
            .iter()
            .all(|v| *v != [0; 32])
    }
    fn independent(self, other: Self) -> bool {
        self.known()
            && other.known()
            && self.operator != other.operator
            && self.device != other.device
            && self.network != other.network
            && self.power != other.power
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MailboxDurabilityV1 {
    Durable,
    Opportunistic,
}
pub struct ReplicaCandidateV1 {
    pub node: [u8; 32],
    /// Admission covers identity, Privacy, resource budget and actual availability.
    pub admitted: bool,
    pub durability: MailboxDurabilityV1,
    /// The node must promise retention through the ORIGINAL object expiry.
    pub retain_until: i64,
    pub available_bytes: usize,
    pub domains: Option<FailureDomainsV1>,
}
pub struct ReplicaPlacementV1 {
    pub durable: Vec<[u8; 32]>,
    pub opportunistic: Option<[u8; 32]>,
    /// Selection only, not StoreReceipt verification or high-availability proof.
    pub durable_shortfall: usize,
}

/// Select at most two independent durable hosts and one opportunistic host.
/// A single node can accept a deposit, but the shortfall must remain visible.
/// Repairs call this with the unchanged expiry and count only verified receipts
/// later; selecting a node never manufactures a stored replica or rewards.
pub fn select_mailbox_replicas_v1(
    candidates: &[ReplicaCandidateV1],
    bytes: usize,
    original_expires: i64,
    now: i64,
) -> Result<ReplicaPlacementV1> {
    ensure!(
        candidates.len() <= 32
            && (1..=65536).contains(&bytes)
            && original_expires > now
            && original_expires
                .checked_sub(now)
                .is_some_and(|v| v <= super::mailbox_store::MAX_TTL),
        "replica budget/lifetime"
    );
    let mut pins = BTreeSet::new();
    ensure!(
        candidates
            .iter()
            .all(|c| c.node != [0; 32] && pins.insert(c.node)),
        "duplicate replica identity"
    );
    let eligible = |c: &&ReplicaCandidateV1| {
        c.admitted && c.retain_until >= original_expires && c.available_bytes >= bytes
    };
    let qualified: Vec<_> = candidates
        .iter()
        .filter(eligible)
        .filter(|c| {
            c.durability == MailboxDurabilityV1::Durable
                && c.domains.is_some_and(FailureDomainsV1::known)
        })
        .collect();
    // Search pairs so one high-ranked but correlated host cannot hide a valid pair.
    let mut durable = Vec::new();
    'outer: for (i, a) in qualified.iter().enumerate() {
        for b in qualified.iter().skip(i + 1) {
            if a.domains.unwrap().independent(b.domains.unwrap()) {
                durable = vec![a.node, b.node];
                break 'outer;
            }
        }
    }
    if durable.is_empty() {
        if let Some(c) = qualified.first() {
            durable.push(c.node);
        }
    }
    let opportunistic = candidates
        .iter()
        .filter(eligible)
        .find(|c| c.durability == MailboxDurabilityV1::Opportunistic)
        .map(|c| c.node);
    Ok(ReplicaPlacementV1 {
        durable_shortfall: 2 - durable.len(),
        durable,
        opportunistic,
    })
}
