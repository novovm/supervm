//! Concrete asset-account authority on the EXISTING immutable native tree.
//!
//! No registry/database, account creation, grant issuance or publication lives
//! here. Signed asset addresses and their compact authority are read at one root;
//! identity associations cannot confer authority. Root trust belongs to the native
//! candidate/journal owner. This immutable view is NOT a spend permit: same-batch
//! nonce/budget consumption must be compiled into the economic effect plan.
use super::{nov_transfer_batch, quoted_transfer::Account, record_pages};
use crate::native_pipeline::state::frontier::{DeclaredAccess, OwnedStateInput};
use crate::native_pipeline::state::tree::{NodeHash, StateChange, MAX_BATCH_READ_KEYS};
use anyhow::{ensure, Context, Result};
use novovm_adapter_api::uca_spend_policy::{SuppliedAssetSpendGrantV2, SuppliedAssetSpendStateV2};
use novovm_exec::uca_spend::SignatureCheckedAssetSpendV2;
use novovm_protocol::uca_spend::{AssetSpendIntentV2, MAX_UCA_ID_BYTES};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;

// Fixed layout bounds, never caller-selected. No raw ML-DSA spend key is stored
// in a grant; its reference is matched to the actually checked signature.
const ACCOUNT_BYTES: usize = 256;
const GRANT_BYTES: usize = 512;
const DOMAIN: &[u8] = b"novovm/asset-spend-state/v2/";

/// Compact authority of one concrete balance account, not an identity record.
/// Identity status/association changes do not mutate these permissions.
/// Encoding it does not establish that its creation was authorized.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetAccountAuthorityV2 {
    pub account: Vec<u8>,
    pub asset: u8,
    pub active: bool,
    pub current_spend_epoch: u64,
    pub management_key_ref: NodeHash,
    pub recovery_key_ref: Option<NodeHash>,
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

impl AssetAccountAuthorityV2 {
    fn validate(&self, expected: &Account) -> Result<()> {
        ensure!(
            self.account == expected.as_bytes()
                && self.asset == 1
                && self.current_spend_epoch != 0
                && self.management_key_ref != [0; 32]
                && self
                    .recovery_key_ref
                    .is_none_or(|key| key != [0; 32] && key != self.management_key_ref),
            "invalid concrete asset account authority"
        );
        Ok(())
    }
}

fn domain(chain: u64, genesis: &NodeHash, kind: &[u8]) -> Result<Vec<u8>> {
    ensure!(chain != 0 && *genesis != [0; 32], "empty UCA state domain");
    Ok([DOMAIN, &chain.to_be_bytes(), genesis, kind].concat())
}

fn account_prefix(chain: u64, genesis: &NodeHash, account: &Account) -> Result<Vec<u8>> {
    let mut key = domain(chain, genesis, b"/account/")?;
    key.push(account.as_bytes().len() as u8);
    key.extend_from_slice(account.as_bytes());
    Ok(key)
}

fn grant_prefix(intent: &AssetSpendIntentV2) -> Result<Vec<u8>> {
    let mut key = domain(intent.chain_id, &intent.genesis, b"/grant/")?;
    // A fixed hash is a state-key identity, NOT a derived balance address.
    key.extend_from_slice(&intent.nonce_identity()?);
    key.extend_from_slice(&intent.spend_key_ref);
    Ok(key)
}

pub(crate) fn grant_write_declarations(intent: &AssetSpendIntentV2) -> Result<Vec<DeclaredAccess>> {
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
    let mut canonical = [0; GRANT_BYTES];
    ensure!(
        rest.is_empty() && postcard::to_slice(&value, &mut canonical)? == bytes,
        "noncanonical UCA state record"
    );
    Ok(value)
}

/// Pure encoding for authorized initialization/transition. The caller must
/// establish asset management authority and retain unrelated identity records.
/// Never apply these changes merely because an incoming packet asks for them.
pub fn account_record_changes(
    chain: u64,
    genesis: &NodeHash,
    record: &AssetAccountAuthorityV2,
) -> Result<Vec<StateChange>> {
    let address = Account::try_from(record.account.as_slice()).map_err(anyhow::Error::msg)?;
    record.validate(&address)?;
    let mut encoded = [0; ACCOUNT_BYTES];
    record_pages::record_changes(
        &account_prefix(chain, genesis, &address)?,
        postcard::to_slice(record, &mut encoded)?,
        ACCOUNT_BYTES,
    )
}

/// Low-frequency identity association only. Spending never reads this index.
/// Its authorized writer must enforce one identity owner per concrete account.
pub fn identity_association_changes(
    chain: u64,
    genesis: &NodeHash,
    identity: &str,
    account: &Account,
) -> Result<Vec<StateChange>> {
    valid_id(identity)?;
    Ok(vec![StateChange::Put {
        key: owner_key(chain, genesis, account)?,
        value: identity.as_bytes().to_vec(),
    }])
}

/// Original identity record, retained outside the ordinary asset-spend read set.
/// This codec has no identity or asset registration authority of its own.
pub fn identity_record_changes(
    chain: u64,
    genesis: &NodeHash,
    identity: &novovm_adapter_api::unified_account::UcaAccount,
) -> Result<Vec<StateChange>> {
    valid_id(&identity.uca_id)?;
    ensure!(
        identity.primary_key_ref.len() == 32 && identity.updated_at >= identity.created_at,
        "invalid identity record"
    );
    let mut prefix = domain(chain, genesis, b"/identity/")?;
    prefix.push(identity.uca_id.len() as u8);
    prefix.extend_from_slice(identity.uca_id.as_bytes());
    record_pages::record_changes(&prefix, &postcard::to_allocvec(identity)?, 8192)
}

pub(crate) fn account_write_declarations(
    chain: u64,
    genesis: &NodeHash,
    account: &Account,
) -> Result<Vec<DeclaredAccess>> {
    record_pages::declarations(
        &account_prefix(chain, genesis, account)?,
        ACCOUNT_BYTES,
        true,
    )
}

pub(crate) fn authority_failure(
    intent: &AssetSpendIntentV2,
    parent: NodeHash,
    height: u64,
    authority: &AssetAccountAuthorityV2,
    grant: Option<&SuppliedAssetSpendGrantV2>,
) -> Option<novovm_adapter_api::uca_spend_policy::SpendPolicyError> {
    novovm_adapter_api::uca_spend_policy::check_supplied_spend_authority_v2(
        intent,
        Some(&SuppliedAssetSpendStateV2 {
            chain_id: intent.chain_id,
            genesis: intent.genesis,
            parent_root: parent,
            account: &authority.account,
            account_active: authority.active,
            grant,
            management_key_ref: authority.management_key_ref,
            recovery_key_ref: authority.recovery_key_ref,
            current_spend_epoch: Some(authority.current_spend_epoch),
            next_spend_nonce: None,
        }),
        &parent,
        height,
    )
    .err()
}

/// Codec only, NOT an issue-grant transaction. Identity and current epoch are
/// checked again on captured-state consumption, not assumed from this helper.
pub fn grant_record_changes(
    intent: &AssetSpendIntentV2,
    grant: &SuppliedAssetSpendGrantV2,
) -> Result<Vec<StateChange>> {
    intent.validate()?;
    ensure!(
        grant.account == intent.payer
            && grant.asset == intent.asset
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

/// Omit unchanged pages only when the caller supplies the grant already decoded
/// from the complete declared range at this same parent. None is not proof of
/// absence: management may replace a record that this batch did not decode.
pub(crate) fn grant_record_changes_from_parent(
    intent: &AssetSpendIntentV2,
    grant: &SuppliedAssetSpendGrantV2,
    previous: Option<(&AssetSpendIntentV2, &SuppliedAssetSpendGrantV2)>,
) -> Result<Vec<StateChange>> {
    let changes = grant_record_changes(intent, grant)?;
    match previous {
        Some((old_intent, old_grant)) => {
            // Epoch is not in the physical prefix, but the original codec checks
            // it. Use the captured record's normalized intent, not the new epoch.
            changed_record_pages(changes, grant_record_changes(old_intent, old_grant)?)
        }
        None => Ok(changes),
    }
}

fn changed_record_pages(
    changes: Vec<StateChange>,
    previous: Vec<StateChange>,
) -> Result<Vec<StateChange>> {
    ensure!(
        changes.len() == previous.len(),
        "record page delta profile mismatch"
    );
    let mut changed = Vec::with_capacity(changes.len());
    for (change, old) in changes.into_iter().zip(previous) {
        let (key, value) = match &change {
            StateChange::Put { key, value } => (key, Some(value)),
            StateChange::Delete { key } => (key, None),
        };
        let (old_key, old_value) = match &old {
            StateChange::Put { key, value } => (key, Some(value)),
            StateChange::Delete { key } => (key, None),
        };
        ensure!(key == old_key, "record page delta prefix mismatch");
        if value != old_value {
            changed.push(change);
        }
    }
    Ok(changed)
}

/// Reuses the account's ordinary grant/nonce namespace. No funding or identity
/// authority is introduced by this descriptor location.
pub(crate) fn creation_location(intent: &AssetSpendIntentV2) -> AssetSpendIntentV2 {
    let mut location = intent.clone();
    location.payer = intent.recipient.clone();
    location.authority_update = None;
    location.recipient_creation = None;
    location.authorization_epoch = 1;
    location
}

pub(crate) fn creation_authority(intent: &AssetSpendIntentV2) -> Option<AssetAccountAuthorityV2> {
    intent
        .recipient_creation
        .as_ref()
        .map(|creation| AssetAccountAuthorityV2 {
            account: intent.recipient.clone(),
            asset: intent.asset,
            active: true,
            current_spend_epoch: 1,
            management_key_ref: creation.management_key_ref,
            recovery_key_ref: creation.recovery_key_ref,
        })
}

pub(crate) fn creation_grants(intent: &AssetSpendIntentV2) -> Vec<SuppliedAssetSpendGrantV2> {
    intent
        .recipient_creation
        .as_ref()
        .map_or_else(Vec::new, |creation| {
            creation
                .grants
                .iter()
                .map(|grant| SuppliedAssetSpendGrantV2 {
                    account: intent.recipient.clone(),
                    asset: intent.asset,
                    key_ref: grant.key_ref,
                    authorization_epoch: 1,
                    permits_nov_transfer: grant.permits_nov_transfer,
                    revoked: false,
                    not_before_height: grant.not_before_height,
                    expires_at_height: grant.expires_at_height,
                    max_debit_per_intent: grant.max_debit_per_intent,
                    remaining_debit_budget: grant.remaining_debit_budget,
                })
                .collect()
        })
}

pub(crate) fn creation_declarations(intent: &AssetSpendIntentV2) -> Result<Vec<DeclaredAccess>> {
    if intent.recipient_creation.is_none() {
        return Ok(Vec::new());
    }
    let mut location = creation_location(intent);
    let account = Account::try_from(intent.recipient.as_slice()).map_err(anyhow::Error::msg)?;
    let mut declarations = account_write_declarations(intent.chain_id, &intent.genesis, &account)?;
    for grant in creation_grants(intent) {
        location.spend_key_ref = grant.key_ref;
        declarations.extend(grant_write_declarations(&location)?);
    }
    for key in [
        nov_transfer_batch::balance_key(&account),
        nov_transfer_batch::nonce_key(&location.nonce_identity()?),
    ] {
        declarations.push(DeclaredAccess {
            key,
            may_put: true,
            may_delete: false,
        });
    }
    Ok(declarations)
}

/// Access declarations for signed concrete asset accounts. The supplied Account
/// values are checked copies of signed bytes, never unsigned address hints.
pub struct AssetSpendStateAccessV2 {
    intent: AssetSpendIntentV2,
    payer: Account,
    recipient: Account,
    declarations: Vec<DeclaredAccess>,
}

impl AssetSpendStateAccessV2 {
    pub fn new(intent: AssetSpendIntentV2, payer: Account, recipient: Account) -> Result<Self> {
        intent.validate()?;
        ensure!(
            intent.payer == payer.as_bytes() && intent.recipient == recipient.as_bytes(),
            "signed asset account mismatch"
        );
        let mut declarations = BTreeMap::<Vec<u8>, DeclaredAccess>::new();
        for address in [&payer, &recipient] {
            for access in record_pages::declarations(
                &account_prefix(intent.chain_id, &intent.genesis, address)?,
                ACCOUNT_BYTES,
                false,
            )? {
                declarations.insert(access.key.clone(), access);
            }
            let key = nov_transfer_batch::balance_key(address);
            declarations.insert(
                key.clone(),
                DeclaredAccess {
                    key,
                    may_put: false,
                    may_delete: false,
                },
            );
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
        for mut declaration in creation_declarations(&intent)? {
            declaration.may_put = false;
            declaration.may_delete = false;
            declarations.insert(declaration.key.clone(), declaration);
        }
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
    ) -> Result<CapturedAssetSpendStateV2> {
        ensure!(
            expected_parent != [0; 32] && input.parent_root() == expected_parent,
            "UCA input parent mismatch"
        );
        let keys: Vec<_> = self.declarations.iter().map(|a| a.key.clone()).collect();
        let values = input.read_many(&keys)?;
        ensure!(values.len() == keys.len(), "UCA capture shape mismatch");
        let records: BTreeMap<_, _> = keys.into_iter().zip(values).collect();
        let read = |key: &[u8]| records.get(key).cloned().context("UCA undeclared input");
        self.read_with(expected_parent, read)
    }

    /// Resolve the union ONCE from the same authenticated frontier. This only
    /// shares immutable leaf material, not the row's epoch/nonce/budget verdict.
    /// The original reader still checks every account, canonical record and
    /// tail page per row. Missing declarations/data remain errors, never zero.
    pub(crate) fn read_captured_batch<'a>(
        accesses: &'a [Self],
        input: &OwnedStateInput,
        expected_parent: NodeHash,
    ) -> Result<impl Iterator<Item = Result<CapturedAssetSpendStateV2>> + 'a> {
        ensure!(
            expected_parent != [0; 32] && input.parent_root() == expected_parent,
            "UCA input parent mismatch"
        );
        let keys = accesses
            .iter()
            .flat_map(|access| access.declarations.iter().map(|d| &d.key))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        // This is the existing tree/plan key bound and its already-reserved
        // key-value materialization allowance, not another state cache.
        ensure!(
            keys.len() <= MAX_BATCH_READ_KEYS,
            "UCA batch read key bound"
        );
        let values = input.read_many(&keys)?;
        ensure!(values.len() == keys.len(), "UCA capture shape mismatch");
        ensure!(
            values
                .iter()
                .all(|value| value.as_ref().is_none_or(|v| v.len() <= 256)),
            "UCA batch read value bound"
        );
        let records: BTreeMap<_, _> = keys.into_iter().zip(values).collect();
        Ok(accesses.iter().map(move |access| {
            access.read_with(expected_parent, |key| {
                records.get(key).cloned().context("UCA undeclared input")
            })
        }))
    }

    /// Read-only adapter for the original asynchronous immutable-tree owner.
    /// The caller pins EVERY supplied read to `expected_parent`; this helper
    /// cannot authenticate a peer-supplied snapshot or issue a spend permit.
    pub(crate) fn read_optional_with(
        &self,
        expected_parent: NodeHash,
        mut read: impl FnMut(&[u8]) -> Result<Option<Vec<u8>>>,
    ) -> Result<Option<CapturedAssetSpendStateV2>> {
        ensure!(expected_parent != [0; 32], "UCA input parent missing");
        // Absence means a complete required record range is absent. Partial
        // records or orphan tail pages remain errors, never an absent account.
        for (index, (prefix, bytes)) in [
            (
                account_prefix(self.intent.chain_id, &self.intent.genesis, &self.payer)?,
                ACCOUNT_BYTES,
            ),
            (
                account_prefix(self.intent.chain_id, &self.intent.genesis, &self.recipient)?,
                ACCOUNT_BYTES,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let declarations = record_pages::declarations(&prefix, bytes, false)?;
            if read(&declarations[0].key)?.is_none() {
                for declaration in declarations.iter().skip(1) {
                    ensure!(read(&declaration.key)?.is_none(), "orphan UCA record page");
                }
                if index == 0 || self.intent.recipient_creation.is_none() {
                    return Ok(None);
                }
            }
        }
        let state = self.read_with(expected_parent, read)?;
        if let Some(grant) = &state.grant {
            ensure!(
                grant.account == self.intent.payer
                    && grant.asset == self.intent.asset
                    && grant.key_ref == self.intent.spend_key_ref,
                "asset grant state identity mismatch"
            );
        }
        Ok(Some(state))
    }

    fn read_with(
        &self,
        expected_parent: NodeHash,
        mut read: impl FnMut(&[u8]) -> Result<Option<Vec<u8>>>,
    ) -> Result<CapturedAssetSpendStateV2> {
        ensure!(expected_parent != [0; 32], "UCA input parent missing");
        let mut accounts = Vec::new();
        let mut balances = Vec::new();
        let mut creates_recipient = false;
        for address in [&self.payer, &self.recipient] {
            let prefix = account_prefix(self.intent.chain_id, &self.intent.genesis, address)?;
            let declarations = record_pages::declarations(&prefix, ACCOUNT_BYTES, false)?;
            if address == &self.recipient
                && self.intent.recipient_creation.is_some()
                && read(&declarations[0].key)?.is_none()
            {
                // Only the explicit opening purpose can consume proved absence.
                // A partial record/balance/nonce/grant must never be reset.
                for declaration in creation_declarations(&self.intent)? {
                    ensure!(
                        read(&declaration.key)?.is_none(),
                        "asset opening has existing or orphan state"
                    );
                }
                accounts
                    .push(creation_authority(&self.intent).context("missing opening descriptor")?);
                balances.push(0);
                creates_recipient = true;
                continue;
            }
            let record: AssetAccountAuthorityV2 = decode(&record_pages::read_with(
                &mut read,
                &account_prefix(self.intent.chain_id, &self.intent.genesis, address)?,
                ACCOUNT_BYTES,
            )?)?;
            record.validate(address)?;
            let balance = read(&nov_transfer_batch::balance_key(address))?
                .context("UCA balance uninitialized")?;
            balances.push(u128::from_le_bytes(
                balance.as_slice().try_into().context("UCA balance shape")?,
            ));
            accounts.push(record);
        }
        let grant_prefix = grant_prefix(&self.intent)?;
        let grant_declarations = record_pages::declarations(&grant_prefix, GRANT_BYTES, false)?;
        let grant = if read(&grant_declarations[0].key)?.is_some() {
            Some(decode(&record_pages::read_with(
                &mut read,
                &grant_prefix,
                GRANT_BYTES,
            )?)?)
        } else {
            for declaration in grant_declarations.iter().skip(1) {
                ensure!(read(&declaration.key)?.is_none(), "orphan asset grant page");
            }
            None
        };
        let nonce = read(&nov_transfer_batch::nonce_key(
            &self.intent.nonce_identity()?,
        ))?
        .context("UCA spend nonce uninitialized")?;
        let next_nonce =
            u64::from_le_bytes(nonce.as_slice().try_into().context("UCA nonce shape")?);
        Ok(CapturedAssetSpendStateV2 {
            intent: self.intent.clone(),
            parent: expected_parent,
            payer: accounts.remove(0),
            recipient: accounts.remove(0),
            grant,
            next_nonce,
            payer_balance: balances[0],
            recipient_balance: balances[1],
            creates_recipient,
        })
    }
}

/// Immutable captured input, not an authorization token or a completed spend.
/// No Deserialize or public constructor can substitute individual state fields.
pub struct CapturedAssetSpendStateV2 {
    intent: AssetSpendIntentV2,
    parent: NodeHash,
    payer: AssetAccountAuthorityV2,
    recipient: AssetAccountAuthorityV2,
    grant: Option<SuppliedAssetSpendGrantV2>,
    next_nonce: u64,
    payer_balance: u128,
    recipient_balance: u128,
    creates_recipient: bool,
}

impl CapturedAssetSpendStateV2 {
    pub(crate) fn matches_intent(&self, intent: &AssetSpendIntentV2) -> bool {
        &self.intent == intent
    }

    pub(crate) fn static_failure(
        &self,
        height: u64,
    ) -> Option<novovm_adapter_api::uca_spend_policy::SpendPolicyError> {
        authority_failure(
            &self.intent,
            self.parent,
            height,
            &self.payer,
            self.grant.as_ref(),
        )
    }
    pub fn parent_root(&self) -> NodeHash {
        self.parent
    }
    pub fn payer(&self) -> &AssetAccountAuthorityV2 {
        &self.payer
    }
    pub fn recipient(&self) -> &AssetAccountAuthorityV2 {
        &self.recipient
    }
    pub fn grant(&self) -> Option<&SuppliedAssetSpendGrantV2> {
        self.grant.as_ref()
    }
    pub fn next_nonce(&self) -> u64 {
        self.next_nonce
    }
    pub fn balances(&self) -> (u128, u128) {
        (self.payer_balance, self.recipient_balance)
    }

    pub(crate) fn creates_recipient(&self) -> bool {
        self.creates_recipient
    }

    pub fn preflight(
        &self,
        signed: &SignatureCheckedAssetSpendV2,
        expected_parent: NodeHash,
        height: u64,
    ) -> Result<()> {
        ensure!(
            signed.intent() == &self.intent,
            "UCA signature/captured intent mismatch"
        );
        signed
            .check_supplied_state(
                Some(&SuppliedAssetSpendStateV2 {
                    chain_id: self.intent.chain_id,
                    genesis: self.intent.genesis,
                    parent_root: self.parent,
                    account: &self.payer.account,
                    account_active: self.payer.active,
                    management_key_ref: self.payer.management_key_ref,
                    recovery_key_ref: self.payer.recovery_key_ref,
                    grant: self.grant.as_ref(),
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
