//! U3 captured-state foundation on the EXISTING immutable native tree.
//!
//! No registry/database, account creation, grant issuance or publication lives
//! here. Account and balance-location hints are verified against the same root;
//! they cannot confer authority. Root trust still belongs to the native
//! candidate/journal owner. This immutable view is NOT a spend permit: same-batch
//! nonce/budget consumption must be compiled into the economic effect plan.
use super::{nov_transfer_batch, quoted_transfer::Account, record_pages};
use crate::native_pipeline::state::frontier::{DeclaredAccess, OwnedStateInput};
use crate::native_pipeline::state::tree::{NodeHash, StateChange};
use anyhow::{ensure, Context, Result};
use novovm_adapter_api::uca_spend_policy::{SuppliedSpendGrantV1, SuppliedSpendStateV1};
use novovm_adapter_api::unified_account::{BindingState, PersonaBinding, PersonaType, UcaAccount};
use novovm_exec::uca_spend::SignatureCheckedUcaSpendV1;
use novovm_protocol::uca_spend::{UcaSpendIntentV1, MAX_UCA_ID_BYTES};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;

// Fixed layout bounds, never caller-selected. No raw ML-DSA spend key is stored
// in a grant; its reference is matched to the actually checked signature.
const ACCOUNT_BYTES: usize = 8192;
const GRANT_BYTES: usize = 512;
const DOMAIN: &[u8] = b"novovm/uca-spend-state/v1/";

/// Versioned projection of the original identity/binding types, not a new UCA.
/// Encoding it does not establish that its creation was authorized.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UcaSpendAccountStateV1 {
    pub account: UcaAccount,
    pub binding: PersonaBinding,
    /// Independent of identity-management delegation_epoch and timestamps.
    pub current_spend_epoch: u64,
}

fn valid_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= MAX_UCA_ID_BYTES
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b)),
        "invalid UCA state identity"
    );
    Ok(())
}

impl UcaSpendAccountStateV1 {
    fn validate(&self, chain: u64, id: &str) -> Result<Account> {
        valid_id(id)?;
        ensure!(
            self.account.uca_id == id
                && self.binding.uca_id == id
                && self.account.primary_key_ref.len() == 32
                && self.account.updated_at >= self.account.created_at
                && self.current_spend_epoch != 0,
            "invalid UCA account projection"
        );
        ensure!(
            self.binding.chain_id == chain
                && self.binding.persona_type == PersonaType::Web30
                && self.binding.binding_state == BindingState::Bound
                && self.binding.revoked_at.is_none()
                && self.binding.cooldown_until.is_none(),
            "UCA NOV binding unavailable"
        );
        Account::try_from(self.binding.external_address.as_slice()).map_err(anyhow::Error::msg)
    }
}

fn domain(chain: u64, genesis: &NodeHash, kind: &[u8]) -> Result<Vec<u8>> {
    ensure!(chain != 0 && *genesis != [0; 32], "empty UCA state domain");
    Ok([DOMAIN, &chain.to_be_bytes(), genesis, kind].concat())
}

fn account_prefix(chain: u64, genesis: &NodeHash, id: &str) -> Result<Vec<u8>> {
    valid_id(id)?;
    let mut key = domain(chain, genesis, b"/account/")?;
    key.push(id.len() as u8);
    key.extend_from_slice(id.as_bytes());
    Ok(key)
}

fn grant_prefix(intent: &UcaSpendIntentV1) -> Result<Vec<u8>> {
    let mut key = domain(intent.chain_id, &intent.genesis, b"/grant/")?;
    // A fixed hash is a state-key identity, NOT a derived balance address.
    key.extend_from_slice(&intent.nonce_identity()?);
    key.extend_from_slice(&intent.spend_key_ref);
    Ok(key)
}

pub(crate) fn grant_write_declarations(intent: &UcaSpendIntentV1) -> Result<Vec<DeclaredAccess>> {
    record_pages::declarations(&grant_prefix(intent)?, GRANT_BYTES, true)
}

fn owner_key(chain: u64, genesis: &NodeHash, account: &Account) -> Result<Vec<u8>> {
    let mut key = domain(chain, genesis, b"/binding-owner/")?;
    key.push(account.as_bytes().len() as u8);
    key.extend_from_slice(account.as_bytes());
    Ok(key)
}

fn decode<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T> {
    let (value, rest): (T, &[u8]) = postcard::take_from_bytes(bytes)?;
    let mut canonical = [0; ACCOUNT_BYTES];
    ensure!(
        rest.is_empty() && postcard::to_slice(&value, &mut canonical)? == bytes,
        "noncanonical UCA state record"
    );
    Ok(value)
}

/// Pure encoding for a future authorized initialization/transition. The caller
/// must enforce existing binding-owner uniqueness and retain unrelated records.
/// Never apply these changes merely because an incoming packet asks for them.
pub fn account_record_changes(
    chain: u64,
    genesis: &NodeHash,
    record: &UcaSpendAccountStateV1,
) -> Result<Vec<StateChange>> {
    let id = &record.account.uca_id;
    let address = record.validate(chain, id)?;
    let mut encoded = [0; ACCOUNT_BYTES];
    let mut changes = record_pages::record_changes(
        &account_prefix(chain, genesis, id)?,
        postcard::to_slice(record, &mut encoded)?,
        ACCOUNT_BYTES,
    )?;
    changes.push(StateChange::Put {
        key: owner_key(chain, genesis, &address)?,
        value: id.as_bytes().to_vec(),
    });
    Ok(changes)
}

/// Codec only, NOT an issue-grant transaction. Identity and current epoch are
/// checked again on captured-state consumption, not assumed from this helper.
pub fn grant_record_changes(
    intent: &UcaSpendIntentV1,
    grant: &SuppliedSpendGrantV1,
) -> Result<Vec<StateChange>> {
    intent.validate()?;
    ensure!(
        grant.account_id == intent.account_id
            && grant.key_ref == intent.spend_key_ref
            && grant.authorization_epoch != 0
            && grant.authorization_epoch == intent.authorization_epoch
            && grant.expires_at_height > grant.not_before_height,
        "invalid UCA grant record identity/window"
    );
    let mut encoded = [0; GRANT_BYTES];
    record_pages::record_changes(
        &grant_prefix(intent)?,
        postcard::to_slice(grant, &mut encoded)?,
        GRANT_BYTES,
    )
}

/// Parent-independent location hints only. The decoded authoritative bindings
/// MUST match these bytes and their reverse owners before they can be consumed.
/// An owner can obtain hints from its existing lookup/capture preparation; this
/// API does not accept a caller-supplied grant or an alternate state reader.
pub struct UcaSpendStateAccessV1 {
    intent: UcaSpendIntentV1,
    payer: Account,
    recipient: Account,
    declarations: Vec<DeclaredAccess>,
}

impl UcaSpendStateAccessV1 {
    pub fn new(intent: UcaSpendIntentV1, payer: Account, recipient: Account) -> Result<Self> {
        intent.validate()?;
        ensure!(
            (intent.account_id == intent.recipient_uca) == (payer == recipient),
            "UCA self/alias binding hints inconsistent"
        );
        let mut declarations = BTreeMap::<Vec<u8>, DeclaredAccess>::new();
        for (id, address) in [
            (&intent.account_id, &payer),
            (&intent.recipient_uca, &recipient),
        ] {
            for access in record_pages::declarations(
                &account_prefix(intent.chain_id, &intent.genesis, id)?,
                ACCOUNT_BYTES,
                false,
            )? {
                declarations.insert(access.key.clone(), access);
            }
            for key in [
                owner_key(intent.chain_id, &intent.genesis, address)?,
                nov_transfer_batch::balance_key(address),
            ] {
                declarations.insert(
                    key.clone(),
                    DeclaredAccess {
                        key,
                        may_put: false,
                        may_delete: false,
                    },
                );
            }
        }
        for access in record_pages::declarations(&grant_prefix(&intent)?, GRANT_BYTES, false)? {
            declarations.insert(access.key.clone(), access);
        }
        let key = nov_transfer_batch::nonce_key(&intent.nonce_identity()?);
        declarations.insert(
            key.clone(),
            DeclaredAccess {
                key,
                may_put: false,
                may_delete: false,
            },
        );
        Ok(Self {
            intent,
            payer,
            recipient,
            declarations: declarations.into_values().collect(),
        })
    }

    pub fn declarations(&self) -> &[DeclaredAccess] {
        &self.declarations
    }

    /// Read-only foundational adapter. `expected_parent` must come from the
    /// existing validated candidate context, not from a peer's claimed root.
    /// No persistence or grant/nonce consumption occurs, even when preflight passes.
    pub fn read(
        &self,
        input: &OwnedStateInput,
        expected_parent: NodeHash,
    ) -> Result<CapturedUcaSpendStateV1> {
        ensure!(
            expected_parent != [0; 32] && input.parent_root() == expected_parent,
            "UCA input parent mismatch"
        );
        let keys: Vec<_> = self.declarations.iter().map(|a| a.key.clone()).collect();
        let values = input.read_many(&keys)?;
        ensure!(values.len() == keys.len(), "UCA capture shape mismatch");
        let records: BTreeMap<_, _> = keys.into_iter().zip(values).collect();
        let read = |key: &[u8]| records.get(key).cloned().context("UCA undeclared input");
        let mut accounts = Vec::new();
        let mut balances = Vec::new();
        for (id, hint) in [
            (&self.intent.account_id, &self.payer),
            (&self.intent.recipient_uca, &self.recipient),
        ] {
            let record: UcaSpendAccountStateV1 = decode(&record_pages::read_with(
                &read,
                &account_prefix(self.intent.chain_id, &self.intent.genesis, id)?,
                ACCOUNT_BYTES,
            )?)?;
            ensure!(
                &record.validate(self.intent.chain_id, id)? == hint,
                "UCA binding hint mismatch"
            );
            let owner = read(&owner_key(
                self.intent.chain_id,
                &self.intent.genesis,
                hint,
            )?)?
            .context("UCA reverse binding owner missing")?;
            ensure!(owner == id.as_bytes(), "UCA asset binding owner mismatch");
            let balance = read(&nov_transfer_batch::balance_key(hint))?
                .context("UCA balance uninitialized")?;
            balances.push(u128::from_le_bytes(
                balance.as_slice().try_into().context("UCA balance shape")?,
            ));
            accounts.push(record);
        }
        let grant = decode(&record_pages::read_with(
            &read,
            &grant_prefix(&self.intent)?,
            GRANT_BYTES,
        )?)?;
        let nonce = read(&nov_transfer_batch::nonce_key(
            &self.intent.nonce_identity()?,
        ))?
        .context("UCA spend nonce uninitialized")?;
        let next_nonce =
            u64::from_le_bytes(nonce.as_slice().try_into().context("UCA nonce shape")?);
        Ok(CapturedUcaSpendStateV1 {
            intent: self.intent.clone(),
            parent: expected_parent,
            payer: accounts.remove(0),
            recipient: accounts.remove(0),
            grant,
            next_nonce,
            payer_balance: balances[0],
            recipient_balance: balances[1],
        })
    }
}

/// Immutable captured input, not an authorization token or a completed spend.
/// No Deserialize or public constructor can substitute individual state fields.
pub struct CapturedUcaSpendStateV1 {
    intent: UcaSpendIntentV1,
    parent: NodeHash,
    payer: UcaSpendAccountStateV1,
    recipient: UcaSpendAccountStateV1,
    grant: SuppliedSpendGrantV1,
    next_nonce: u64,
    payer_balance: u128,
    recipient_balance: u128,
}

impl CapturedUcaSpendStateV1 {
    pub(crate) fn static_failure(
        &self,
        height: u64,
    ) -> Option<novovm_adapter_api::uca_spend_policy::SpendPolicyError> {
        novovm_adapter_api::uca_spend_policy::check_supplied_spend_authority_v1(
            &self.intent,
            Some(&SuppliedSpendStateV1 {
                chain_id: self.intent.chain_id,
                genesis: self.intent.genesis,
                parent_root: self.parent,
                account: &self.payer.account,
                grant: &self.grant,
                current_spend_epoch: Some(self.payer.current_spend_epoch),
                next_spend_nonce: Some(self.next_nonce),
            }),
            &self.parent,
            height,
        )
        .err()
    }
    pub fn parent_root(&self) -> NodeHash {
        self.parent
    }
    pub fn payer(&self) -> &UcaSpendAccountStateV1 {
        &self.payer
    }
    pub fn recipient(&self) -> &UcaSpendAccountStateV1 {
        &self.recipient
    }
    pub fn grant(&self) -> &SuppliedSpendGrantV1 {
        &self.grant
    }
    pub fn next_nonce(&self) -> u64 {
        self.next_nonce
    }
    pub fn balances(&self) -> (u128, u128) {
        (self.payer_balance, self.recipient_balance)
    }

    pub fn preflight(
        &self,
        signed: &SignatureCheckedUcaSpendV1,
        expected_parent: NodeHash,
        height: u64,
    ) -> Result<()> {
        ensure!(
            signed.intent() == &self.intent,
            "UCA signature/captured intent mismatch"
        );
        signed
            .check_supplied_state(
                Some(&SuppliedSpendStateV1 {
                    chain_id: self.intent.chain_id,
                    genesis: self.intent.genesis,
                    parent_root: self.parent,
                    account: &self.payer.account,
                    grant: &self.grant,
                    current_spend_epoch: Some(self.payer.current_spend_epoch),
                    next_spend_nonce: Some(self.next_nonce),
                }),
                &expected_parent,
                height,
            )
            .map_err(|e| anyhow::anyhow!("captured UCA preflight: {e:?}"))
    }
}

#[cfg(test)]
mod tests;
