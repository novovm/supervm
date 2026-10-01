//! Governance module - nine-member weighted council
//!
//! Composition:
//! - Founder 1 seat (35%)
//! - Top 5 holders 5 seats (10% each, total 50%)
//! - Team representatives 2 seats (5% each, total 10%)
//! - Independent member 1 seat (5%)
//!
//! Decision thresholds:
//! - Parameter change: >50%
//! - Treasury spend: >66%
//! - Protocol upgrade: >75%
//! - Emergency freeze: >50% + at least 3 distinct seat categories

use crate::types::Address;
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};

/// Council seat types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CouncilSeat {
    /// Founder (35% voting power)
    Founder,
    /// One of top 5 holders (10% each)
    TopHolder(u8), // 0-4
    /// Team representative (5% each)
    Team(u8), // 0-1
    /// Independent member (5%)
    Independent,
}

impl CouncilSeat {
    /// Get seat voting weight (bps, 10000 = 100%)
    pub fn voting_weight(&self) -> u16 {
        match self {
            CouncilSeat::Founder => 3500,      // 35%
            CouncilSeat::TopHolder(_) => 1000, // 10%
            CouncilSeat::Team(_) => 500,       // 5%
            CouncilSeat::Independent => 500,   // 5%
        }
    }

    /// Get seat category (for emergency proposal diversity check)
    pub fn category(&self) -> &str {
        match self {
            CouncilSeat::Founder => "Founder",
            CouncilSeat::TopHolder(_) => "TopHolder",
            CouncilSeat::Team(_) => "Team",
            CouncilSeat::Independent => "Independent",
        }
    }
}

/// Proposal types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalType {
    /// Parameter change (gas fee/burn ratio/unlock speed)
    ParameterChange,
    /// Treasury spend (ecosystem investment/team incentives)
    TreasurySpend,
    /// Protocol upgrade (hard fork/major change)
    ProtocolUpgrade,
    /// Emergency freeze (security incident response)
    EmergencyFreeze,
}

impl ProposalType {
    /// Get passing threshold (bps)
    pub fn passing_threshold(&self) -> u16 {
        match self {
            ProposalType::ParameterChange => 5000, // >50%
            ProposalType::TreasurySpend => 6600,   // >66%
            ProposalType::ProtocolUpgrade => 7500, // >75%
            ProposalType::EmergencyFreeze => 5000, // >50%
        }
    }

    /// Whether diversity check across categories is required
    pub fn requires_diversity(&self) -> bool {
        matches!(self, ProposalType::EmergencyFreeze)
    }

    /// Minimum number of categories for diversity check
    pub fn min_categories(&self) -> usize {
        match self {
            ProposalType::EmergencyFreeze => 3,
            _ => 0,
        }
    }
}

/// Vote options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vote {
    Approve,
    Reject,
    Abstain,
}

/// Proposal status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalStatus {
    Pending,  // voting
    Passed,   // passed
    Rejected, // rejected
    Executed, // executed
    Expired,  // expired
}

/// Proposal
#[derive(Debug, Clone)]
pub struct Proposal {
    /// Proposal ID
    pub id: u64,
    /// Proposal type
    pub proposal_type: ProposalType,
    /// Proposer
    pub proposer: Address,
    /// Title
    pub title: String,
    /// Description
    pub description: String,
    /// Payload (params/contract calls etc.)
    pub data: Vec<u8>,
    /// Created timestamp
    pub created_at: u64,
    /// Voting deadline timestamp
    pub voting_deadline: u64,
    /// Votes (seat -> vote)
    pub votes: HashMap<CouncilSeat, Vote>,
    /// Status
    pub status: ProposalStatus,
}

/// Vote delegation
#[derive(Debug, Clone)]
pub struct VoteDelegation {
    /// Delegator (small holder)
    pub delegator: Address,
    /// Delegatee (large holder/institution)
    pub delegatee: Address,
    /// Delegated token amount
    pub amount: u128,
    /// Delegated timestamp
    pub delegated_at: u64,
}

/// Governance events
#[derive(Debug, Clone)]
pub enum GovernanceEvent {
    /// Proposal created
    ProposalCreated {
        id: u64,
        proposer: Address,
        proposal_type: ProposalType,
        title: String,
    },
    /// Voted
    Voted {
        proposal_id: u64,
        seat: CouncilSeat,
        voter: Address,
        vote: Vote,
    },
    /// Proposal finalized
    ProposalFinalized {
        id: u64,
        status: ProposalStatus,
        approve_weight: u16,
        reject_weight: u16,
        abstain_weight: u16,
    },
    /// Proposal executed
    ProposalExecuted { id: u64, executor: Address },
    /// Vote delegated
    VoteDelegated {
        delegator: Address,
        delegatee: Address,
        amount: u128,
    },
    /// Delegation revoked
    DelegationRevoked {
        delegator: Address,
        delegatee: Address,
    },
    /// Council member changed
    CouncilMemberChanged {
        seat: CouncilSeat,
        old_member: Option<Address>,
        new_member: Address,
    },
}

/// Governance interface
pub trait Governance {
    /// Create proposal
    fn create_proposal(
        &mut self,
        proposer: &Address,
        proposal_type: ProposalType,
        title: String,
        description: String,
        data: Vec<u8>,
        voting_period: u64, // voting duration (seconds)
    ) -> Result<GovernanceEvent>;

    /// Vote
    fn vote(&mut self, proposal_id: u64, voter: &Address, vote: Vote) -> Result<GovernanceEvent>;

    /// Tally votes
    fn tally_votes(&self, proposal_id: u64) -> Result<(u16, u16, u16)>; // (approve, reject, abstain)

    /// Finalize proposal (after voting deadline)
    fn finalize_proposal(&mut self, proposal_id: u64) -> Result<GovernanceEvent>;

    /// Execute a passed proposal
    fn execute_proposal(&mut self, proposal_id: u64, executor: &Address)
        -> Result<GovernanceEvent>;

    /// Delegate voting power
    fn delegate_vote(
        &mut self,
        delegator: &Address,
        delegatee: &Address,
        amount: u128,
    ) -> Result<GovernanceEvent>;

    /// Revoke vote delegation
    fn revoke_delegation(
        &mut self,
        delegator: &Address,
        delegatee: &Address,
    ) -> Result<GovernanceEvent>;

    /// Get council member address
    fn get_council_member(&self, seat: CouncilSeat) -> Option<Address>;

    /// Set council member address
    fn set_council_member(&mut self, seat: CouncilSeat, member: Address)
        -> Result<GovernanceEvent>;

    /// Get proposal
    fn get_proposal(&self, id: u64) -> Option<&Proposal>;

    /// Get seats held by address
    fn get_seats_by_address(&self, addr: &Address) -> Vec<CouncilSeat>;

    /// Get delegations by delegator
    fn get_delegations(&self, delegator: &Address) -> Vec<&VoteDelegation>;

    /// Get total delegated voting power for delegatee
    fn get_delegated_power(&self, delegatee: &Address) -> u128;
}

/// Governance implementation
pub struct GovernanceImpl {
    /// Council members mapping (seat -> address)
    council_members: HashMap<CouncilSeat, Address>,
    /// Reverse mapping (address -> seats)
    member_seats: HashMap<Address, Vec<CouncilSeat>>,
    /// Proposals
    proposals: HashMap<u64, Proposal>,
    /// Next proposal ID
    next_proposal_id: u64,
    /// Vote delegations
    delegations: Vec<VoteDelegation>,
    /// Delegated power cache (delegatee -> total_amount)
    delegated_power: HashMap<Address, u128>,
}

impl GovernanceImpl {
    pub fn new() -> Self {
        Self {
            council_members: HashMap::new(),
            member_seats: HashMap::new(),
            proposals: HashMap::new(),
            next_proposal_id: 1,
            delegations: Vec::new(),
            delegated_power: HashMap::new(),
        }
    }

    /// Internal helper: get current timestamp
    fn now(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Internal helper: verify voter eligibility
    fn verify_voter(&self, proposal_id: u64, voter: &Address) -> Result<CouncilSeat> {
        let seats = self.get_seats_by_address(voter);
        if seats.is_empty() {
            return Err(anyhow!("Voter is not a council member"));
        }

        // Check if already voted
        let proposal = self
            .proposals
            .get(&proposal_id)
            .ok_or_else(|| anyhow!("Proposal not found"))?;

        for seat in &seats {
            if proposal.votes.contains_key(seat) {
                return Err(anyhow!("Seat {:?} already voted", seat));
            }
        }

        // Return first seat (subsequent votes may use other seats)
        Ok(seats[0])
    }
}

impl Default for GovernanceImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl Governance for GovernanceImpl {
    fn create_proposal(
        &mut self,
        proposer: &Address,
        proposal_type: ProposalType,
        title: String,
        description: String,
        data: Vec<u8>,
        voting_period: u64,
    ) -> Result<GovernanceEvent> {
        // Verify proposer is council member
        if self.get_seats_by_address(proposer).is_empty() {
            return Err(anyhow!("Only council members can create proposals"));
        }

        let id = self.next_proposal_id;
        self.next_proposal_id += 1;

        let now = self.now();
        let proposal = Proposal {
            id,
            proposal_type,
            proposer: *proposer,
            title: title.clone(),
            description,
            data,
            created_at: now,
            voting_deadline: now + voting_period,
            votes: HashMap::new(),
            status: ProposalStatus::Pending,
        };

        self.proposals.insert(id, proposal);

        Ok(GovernanceEvent::ProposalCreated {
            id,
            proposer: *proposer,
            proposal_type,
            title,
        })
    }

    fn vote(&mut self, proposal_id: u64, voter: &Address, vote: Vote) -> Result<GovernanceEvent> {
        let seat = self.verify_voter(proposal_id, voter)?;

        let now = self.now();
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or_else(|| anyhow!("Proposal not found"))?;

        // Check voting deadline
        if now > proposal.voting_deadline {
            return Err(anyhow!("Voting period has ended"));
        }

        if proposal.status != ProposalStatus::Pending {
            return Err(anyhow!("Proposal is not in pending status"));
        }

        // Record vote
        proposal.votes.insert(seat, vote);

        Ok(GovernanceEvent::Voted {
            proposal_id,
            seat,
            voter: *voter,
            vote,
        })
    }

    fn tally_votes(&self, proposal_id: u64) -> Result<(u16, u16, u16)> {
        let proposal = self
            .proposals
            .get(&proposal_id)
            .ok_or_else(|| anyhow!("Proposal not found"))?;

        let mut approve = 0u16;
        let mut reject = 0u16;
        let mut abstain = 0u16;

        for (seat, vote) in &proposal.votes {
            let weight = seat.voting_weight();
            match vote {
                Vote::Approve => approve += weight,
                Vote::Reject => reject += weight,
                Vote::Abstain => abstain += weight,
            }
        }

        Ok((approve, reject, abstain))
    }

    fn finalize_proposal(&mut self, proposal_id: u64) -> Result<GovernanceEvent> {
        let now = self.now();
        let (voting_deadline, current_status, proposal_type, votes_snapshot) = {
            let proposal = self
                .proposals
                .get(&proposal_id)
                .ok_or_else(|| anyhow!("Proposal not found"))?;
            (
                proposal.voting_deadline,
                proposal.status,
                proposal.proposal_type,
                proposal.votes.clone(),
            )
        };

        // Check deadline reached
        if now <= voting_deadline {
            return Err(anyhow!("Voting period has not ended"));
        }

        if current_status != ProposalStatus::Pending {
            return Err(anyhow!("Proposal already finalized"));
        }

        let (approve, reject, abstain) = self.tally_votes(proposal_id)?;
        let threshold = proposal_type.passing_threshold();

        // Check passing threshold
        let mut passed = approve > threshold;

        // Diversity check (emergency proposals)
        if passed && proposal_type.requires_diversity() {
            let mut categories: HashSet<String> = HashSet::new();
            for (seat, vote) in votes_snapshot.iter() {
                if *vote == Vote::Approve {
                    categories.insert(seat.category().to_string());
                }
            }
            if categories.len() < proposal_type.min_categories() {
                passed = false; // diversity requirement not met
            }
        }

        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or_else(|| anyhow!("Proposal not found"))?;

        proposal.status = if passed {
            ProposalStatus::Passed
        } else {
            ProposalStatus::Rejected
        };

        Ok(GovernanceEvent::ProposalFinalized {
            id: proposal_id,
            status: proposal.status,
            approve_weight: approve,
            reject_weight: reject,
            abstain_weight: abstain,
        })
    }

    fn execute_proposal(
        &mut self,
        proposal_id: u64,
        executor: &Address,
    ) -> Result<GovernanceEvent> {
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or_else(|| anyhow!("Proposal not found"))?;

        if proposal.status != ProposalStatus::Passed {
            return Err(anyhow!("Proposal has not passed"));
        }

        // Execute proposal logic (based on type and data)
        // In production, call appropriate contracts/functions
        // execute_proposal_data(&proposal.data)?;

        proposal.status = ProposalStatus::Executed;

        Ok(GovernanceEvent::ProposalExecuted {
            id: proposal_id,
            executor: *executor,
        })
    }

    fn delegate_vote(
        &mut self,
        delegator: &Address,
        delegatee: &Address,
        amount: u128,
    ) -> Result<GovernanceEvent> {
        // 验证委托者不是议会成员
        if !self.get_seats_by_address(delegator).is_empty() {
            return Err(anyhow!("Council members cannot delegate"));
        }

        // 记录委托
        self.delegations.push(VoteDelegation {
            delegator: *delegator,
            delegatee: *delegatee,
            amount,
            delegated_at: self.now(),
        });

        // 更新被委托权重
        *self.delegated_power.entry(*delegatee).or_insert(0) += amount;

        Ok(GovernanceEvent::VoteDelegated {
            delegator: *delegator,
            delegatee: *delegatee,
            amount,
        })
    }

    fn revoke_delegation(
        &mut self,
        delegator: &Address,
        delegatee: &Address,
    ) -> Result<GovernanceEvent> {
        // 查找并移除委托
        let index = self
            .delegations
            .iter()
            .position(|d| d.delegator == *delegator && d.delegatee == *delegatee)
            .ok_or_else(|| anyhow!("Delegation not found"))?;

        let delegation = self.delegations.remove(index);

        // 更新被委托权重
        if let Some(power) = self.delegated_power.get_mut(delegatee) {
            *power = power.saturating_sub(delegation.amount);
        }

        Ok(GovernanceEvent::DelegationRevoked {
            delegator: *delegator,
            delegatee: *delegatee,
        })
    }

    fn get_council_member(&self, seat: CouncilSeat) -> Option<Address> {
        self.council_members.get(&seat).copied()
    }

    fn set_council_member(
        &mut self,
        seat: CouncilSeat,
        member: Address,
    ) -> Result<GovernanceEvent> {
        let old_member = self.council_members.insert(seat, member);

        // Update reverse mapping
        if let Some(old) = old_member {
            if let Some(seats) = self.member_seats.get_mut(&old) {
                seats.retain(|&s| s != seat);
            }
        }

        self.member_seats.entry(member).or_default().push(seat);

        Ok(GovernanceEvent::CouncilMemberChanged {
            seat,
            old_member,
            new_member: member,
        })
    }

    fn get_proposal(&self, id: u64) -> Option<&Proposal> {
        self.proposals.get(&id)
    }

    fn get_seats_by_address(&self, addr: &Address) -> Vec<CouncilSeat> {
        self.member_seats.get(addr).cloned().unwrap_or_default()
    }

    fn get_delegations(&self, delegator: &Address) -> Vec<&VoteDelegation> {
        self.delegations
            .iter()
            .filter(|d| d.delegator == *delegator)
            .collect()
    }

    fn get_delegated_power(&self, delegatee: &Address) -> u128 {
        self.delegated_power.get(delegatee).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voting_weights() {
        assert_eq!(CouncilSeat::Founder.voting_weight(), 3500);
        assert_eq!(CouncilSeat::TopHolder(0).voting_weight(), 1000);
        assert_eq!(CouncilSeat::Team(0).voting_weight(), 500);
        assert_eq!(CouncilSeat::Independent.voting_weight(), 500);

        // 总权重应为 10000 (100%)
        let total: u16 = CouncilSeat::Founder.voting_weight()
            + (0..5)
                .map(|i| CouncilSeat::TopHolder(i).voting_weight())
                .sum::<u16>()
            + (0..2)
                .map(|i| CouncilSeat::Team(i).voting_weight())
                .sum::<u16>()
            + CouncilSeat::Independent.voting_weight();
        assert_eq!(total, 10000);
    }

    #[test]
    fn test_proposal_thresholds() {
        assert_eq!(ProposalType::ParameterChange.passing_threshold(), 5000);
        assert_eq!(ProposalType::TreasurySpend.passing_threshold(), 6600);
        assert_eq!(ProposalType::ProtocolUpgrade.passing_threshold(), 7500);
    }
}
