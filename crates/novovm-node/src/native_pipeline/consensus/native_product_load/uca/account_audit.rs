//! Complete, untimed account audit through the original authenticated tree reader.
use super::*;
use crate::native_pipeline::{
    business::{nov_transfer_batch::balance_key, uca_spend_state::AssetSpendStateAccessV2},
    state::tree::read_state_values,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn audit_all_accounts(
    nodes: &ProductNodes,
    head: Status,
    shape: WorkloadShape,
    expected: &[(Account, u128)],
    initial_accounts: &[UcaGenesisAccountV2],
    rounds: u64,
    make_intent: &impl Fn(usize) -> Result<AssetSpendIntentV3>,
) -> Result<Value> {
    ensure!(
        initial_accounts.len() == shape.accounts() && expected.len() == shape.accounts() + 1,
        "complete account audit coverage"
    );
    let mut reference = None;
    for node in 0..4 {
        let store = stopped_store(nodes, node)?;
        let mut digest = Sha256::new();
        digest.update(b"novovm/load/complete-account-economics/v1\0");
        for first in (0..shape.accounts()).step_by(128) {
            let end = (first + 128).min(shape.accounts());
            let accesses = (first..end)
                .map(|index| {
                    let mut intent = make_intent(index)?;
                    // Outside the TPS clock, explicitly read this key's grant
                    // location. Controller admission intentionally omits it.
                    intent.authority = AssetSpendAuthorityV3::DelegatedGrant;
                    AssetSpendStateAccessV2::new(
                        intent.clone(),
                        Account::try_from(intent.payer.as_slice()).map_err(anyhow::Error::msg)?,
                        Account::try_from(intent.recipient.as_slice())
                            .map_err(anyhow::Error::msg)?,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let keys = accesses
                .iter()
                .flat_map(|access| access.declarations().iter().map(|d| d.key.clone()))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let values = read_state_values(&store, head.state_root, &keys)?;
            let records = keys.into_iter().zip(values).collect::<BTreeMap<_, _>>();
            for (index, access) in (first..end).zip(accesses) {
                let state = access
                    .read_optional_with(head.state_root, |key| {
                        records
                            .get(key)
                            .cloned()
                            .context("account audit missing declaration")
                    })?
                    .context("account audit missing initialized account")?;
                let initial = &initial_accounts[index];
                let (balance, recipient_balance) = state.balances();
                let expected_nonce = if index < shape.payers { rounds } else { 0 };
                let intent = make_intent(index)?;
                ensure!(
                    state.parent_root() == head.state_root
                        && state.payer().account == account_bytes(index)
                        && state.payer().asset == 1
                        && state.payer().active
                        && state.payer().current_spend_epoch == 1
                        && state.payer().management_key_ref
                            == spend_key_ref_v1(&initial.management_public_key)?
                        && state.payer().recovery_key_ref.is_none()
                        && initial.grants.is_empty()
                        && state.grant().is_none()
                        && intent.authority == AssetSpendAuthorityV3::Controller
                        && intent.spend_key_ref == state.payer().management_key_ref
                        && state.next_nonce() == expected_nonce
                        && balance == expected[index].1
                        && recipient_balance == expected[shape.payers].1,
                    "complete account authority/balance/nonce mismatch node={node} account={index}"
                );
                digest.update(postcard::to_allocvec(&(
                    state.payer(),
                    state.grant(),
                    state.next_nonce(),
                    balance,
                ))?);
            }
        }
        let treasury = expected.last().unwrap();
        let key = balance_key(&treasury.0);
        let value = read_state_values(&store, head.state_root, &[key])?
            .pop()
            .flatten()
            .context("account audit treasury missing")?;
        ensure!(
            value == treasury.1.to_le_bytes(),
            "complete account audit treasury changed"
        );
        digest.update(value);
        let hash: Hash = digest.finalize().into();
        if let Some(reference) = reference {
            ensure!(
                hash == reference,
                "four-store complete account state differs"
            );
        } else {
            reference = Some(hash);
        }
    }
    Ok(
        json!({"all_four_stores_match":true,"accounts_checked_per_store":shape.accounts(),
        "payer_accounts":shape.payers,"recipient_accounts":shape.recipients(),
        "economic_state_sha256":hex(&reference.context("complete audit produced no hash")?),
        "spend_authority":"Controller","restricted_grants":false,
        "read_batch_accounts":128,"scope":"every payer and recipient controller, absent grant, nonce and balance plus supply remainder; original immutable tree at final published root; outside TPS clock"}),
    )
}
