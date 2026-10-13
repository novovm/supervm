//! Compile component dependencies before execution; never advance balances
//! between native calls. This remains prediction, NOT global fee settlement.
use super::*;
use novovm_exec::resident::{AoemInteger1024V1, AoemIntegerInputV1};

fn literal(n: u128) -> AoemIntegerInputV1 {
    AoemIntegerInputV1::Literal(AoemInteger1024V1::from_u128(n))
}

pub(super) fn speculate(
    prepared: &Prepared,
    resources: &mut ResidentTransferResources,
    now: u128,
) -> Result<IntegerPredictions> {
    ensure!(
        !prepared.components.is_empty() && prepared.requests.len() <= resources.capacity(),
        "NOV dataflow capacity"
    );
    let quotation = FeeQuotationContext::new(&prepared.policy, now)?;
    let mut rows: Vec<Vec<AoemIntegerInputV1>> = Vec::with_capacity(prepared.requests.len());
    let mut metadata = Vec::with_capacity(prepared.requests.len());
    let mut depths = Vec::with_capacity(prepared.requests.len());
    for (component, indices) in prepared.components.iter().enumerate() {
        // Symbol tables contain references, never computed after-balances.
        let mut balances = BTreeMap::new();
        let mut nonces = BTreeMap::new();
        for &index in indices {
            let request = &prepared.requests[index];
            let fee = quotation.prepare(request)?;
            let (intent, rejection) = intent(prepared, index, &fee)?;
            let payer = *balances
                .get(&request.payer)
                .unwrap_or(&literal(prepared.balances[&request.payer].unwrap_or(0)));
            let recipient = *balances
                .get(&request.recipient)
                .unwrap_or(&literal(prepared.balances[&request.recipient].unwrap_or(0)));
            let nonce = *nonces.get(&prepared.identities[index]).unwrap_or(&literal(
                prepared.nonces[&prepared.identities[index]] as u128,
            ));
            let row = vec![
                payer,
                recipient,
                literal(intent.amount),
                literal(intent.approved_fee),
                literal(intent.fee_cap),
                literal(intent.nonce as u128),
                nonce,
                literal(1),
                literal((intent.from == intent.to) as u128),
                literal(rejection.is_some() as u128),
            ];
            let depth = row
                .iter()
                .filter_map(|v| match v {
                    AoemIntegerInputV1::Output { row, .. } => Some(depths[*row as usize] + 1),
                    _ => None,
                })
                .max()
                .unwrap_or(0usize);
            let output = |column| AoemIntegerInputV1::Output {
                row: rows.len() as u32,
                column,
            };
            balances.insert(request.payer.clone(), output(0));
            balances.insert(request.recipient.clone(), output(1));
            nonces.insert(prepared.identities[index], output(2));
            rows.push(row);
            depths.push(depth);
            metadata.push((component, index, intent, rejection, fee));
        }
    }
    ensure!(
        rows.len() == prepared.requests.len(),
        "NOV dataflow coverage"
    );
    // Exactly one delegation. The node sees no intermediate wave results.
    let batch = resources.run_plan(&rows)?;
    let value = |binding: AoemIntegerInputV1| -> Result<u128> {
        match binding {
            AoemIntegerInputV1::Literal(v) => v.try_to_u128(),
            AoemIntegerInputV1::Output { row, column } => {
                batch.rows[row as usize].values[column as usize].try_to_u128()
            }
        }
    };
    let mut components: Vec<Vec<Prediction>> = prepared
        .components
        .iter()
        .map(|c| Vec::with_capacity(c.len()))
        .collect();
    for (row, (component, index, intent, rejection, fee)) in metadata.into_iter().enumerate() {
        // Reconstruct the before-state for canonical receipts AFTER completion;
        // these reads neither decide nor resubmit any numerical work.
        let snapshot = TransferSnapshot {
            payer_balance: value(rows[row][0])?,
            recipient_balance: value(rows[row][1])?,
            next_nonce: u64::try_from(value(rows[row][6])?)?,
        };
        let values = batch.rows[row]
            .values
            .iter()
            .map(|v| v.try_to_u128())
            .collect::<Result<Vec<_>>>()?
            .try_into()
            .ok()
            .context("NOV dataflow output shape")?;
        let outcome = TransferOutcome::from_checked_integer_values(
            &intent,
            &snapshot,
            rejection.as_deref(),
            values,
        )?;
        components[component].push(Prediction {
            index,
            snapshot,
            outcome,
            fee,
        });
    }
    Ok(IntegerPredictions {
        components: components.into_iter().map(Some).collect(),
        rows: rows.len(),
        waves: depths.into_iter().max().unwrap_or(0) + 1,
    })
}
