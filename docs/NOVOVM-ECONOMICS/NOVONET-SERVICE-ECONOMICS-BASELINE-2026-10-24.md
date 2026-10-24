# NOVONET service economics: synchronized baseline

Date: 2026-10-24. **Design decisions updated; implementation not delivered.** Documentation only: no billing activation, supply/configuration changes, asset operations, deployment, or mainnet authorization.

The [Chinese baseline](../../docs_CN/NOVOVM-ECONOMICS/NOVONET-SERVICE-ECONOMICS-BASELINE-2026-10-24.md) is the shared rule source. This is its English synchronization, not a separate economic system. See the [English index](README.md) and [Chinese index](../../docs_CN/NOVOVM-ECONOMICS/README.md). Product behavior is documented in [NOVOCHAT planning](../../../NOVOCHAT/docs/decisions/2026-10-23-uca-pq-account-planning.md).

## Confirmed rules and supersession

| Previous rule | Current decision |
| --- | --- |
| Mandatory monthly package / monthly balance reset | Default NOV pay-as-you-go; funded balances and settled earnings do not expire at month end |
| Minimum USD 5-equivalent founder share per paid month | Cancelled; no such minimum share or minimum monthly spend |
| Fixed package resources | Still apply to awarded free trials and any future optional package; not the default payment model |
| Ambiguous inflation proposal | Moderate service-price adjustments only; no change to fixed native NOV supply |
| Native NOV emission and fee burn | Target: all 100,000,000 NOV issued at genesis; subsequent transfers/unlocks only, no mint or burn |
| 50% treasury or 80/10/10 | Comparison/candidate scenarios, not approved fee parameters |
| UTC storage implies UTC calendar month | Separate timestamp storage and billing calendar; fixed billing timezone is recommended, not finalized |
| Online nodes automatically earn | Only verified contracted service, including purchased verifiable reserved capacity, earns compensation |

These supersede conflicting current requirements in the October 13 contribution draft, October 14 billing contract and older monetary documents. Existing code and historical measurements remain evidence of their actual versions, not proof of the new design. M2 issuance/redemption, liabilities and custody segregation remain in force.

## Supply, resources and balances

- All native NOV is issued at genesis, including locked allocations. Unlocking, treasury subsidies, airdrops and founders selling owned balances do not create new NOV. Circulating supply is reported separately from total supply.
- The [first-release economic contract](NOVOVM-NOV-FIRST-RELEASE-ECONOMIC-CONTRACT-2026-10-24.md) specifies nine decimal places: `1 NOV = 10^9` atomic units, genesis total `10^17`. This is a confirmed design, not end-to-end integer settlement acceptance.
- Other assets such as NETH retain their own issuance/redemption and backing rules. Burning a claim to release its backing is not native NOV destruction. Customer custody, LP assets and founder personal holdings are not discretionary treasury cash.
- NOV is NOVOCHAT's payment and settlement asset. Resource entitlements are not a transferable second token. General NOVOVM multi-asset clearing does not authorize foreign-asset payment or automatic conversion in the App.
- Meter effective bytes, byte-seconds and explicitly contracted compute, routing or capacity services. L1/L2 may earn for selected discovery, routing or relay work; traffic need not traverse every layer. No per-layer tax, paid retransmission loops or duplicate cost categories.
- `GiB = 2^30 bytes`; `GiB-hour = 2^30 * 3600 byte-seconds`. Metering layer, padding, replica/fanout limits and rounding must be specified in the quote. Accumulate micro-usage before atomic-unit settlement; no per-packet upward rounding or per-message chain calls.
- Topups and settled earnings persist. Pending income is not spendable; reservations cannot be spent again. Income never increases the authorized spending cap or automatically funds another UCA. Budget-cycle renewal does not release existing reservations/debts. Optional package rollover/refund terms require separate definition.
- Chat authority is not asset-spending authority. The asset owner may approve bounded automatic spending by UCA, purpose, amount, duration and service scope. Its concrete interface and hardware-wallet integration remain unimplemented.

## Service obligations and settlement

Accepting a task atomically reserves its funded maximum cost and binds provider, payee account, quote and obligation. Verified performance earns the contracted increment. Purchased, verifiable capacity can be work even before it carries traffic; online status or self-reported capacity cannot.

A DeviceReceipt means authenticated durable device delivery. Transport ACKs and StoreReceipts do not mean delivery; a StoreReceipt is a custody commitment, not evidence that its whole future TTL has been performed. Final nondelivery does not make all completed forwarding/storage free. Unperformed portions follow the original refund/dispute terms.

Retries and duplicate receipts cannot charge twice. New replicas require covered obligations. Timeouts do not instantly free unsettled funds while late valid claims remain possible. Preserve:

`final funding = unallocated + active reservations + performed/unsettled + settled + refunded`

These categories are disjoint. Existing funded custody, retrieval and receipt obligations survive later quote/trial/package expiry. Insufficient funds stop new unapproved obligations, not old promises or privacy checks.

Topups, earnings receipts and month-end carryover are not repeated fee-split events. Service shares attach to a defined service economic event, not all network transfers or existing reserve/fee/risk buckets. Provider pool growth is not payment to a concrete provider account.

## Trials, airdrops and calendar boundaries

The free trial is one **calendar month** with fixed awarded resources. Secure funding, capacity and responsible providers before activation. Preserve the campaign direction through April 2027; exact cutoff, timezone, slots, resource amounts and budgets remain pending. Campaign closure cannot shorten an awarded trial.

Recommendation: UTC timestamps with a fixed billing timezone and original day/time anchor; clamp short months without losing that anchor. January 31 at 00:30 in Shanghai is January 30 at 16:30 UTC: adding a month in UTC can land on March 1 locally. UTC storage is not a UTC billing calendar, and 30 days is not a calendar month. Timezone and DST gap/overlap rules need approval.

Trials pay providers from existing funded promotion balances; do not recursively tax treasury-sponsored work back into treasury. Real NOV airdrops are separate from noncash resource rights. Reserve the entire granted staged award, not just its first installment. Unawarded budgets and already granted obligations differ.

UCA uniqueness is not human uniqueness. Eligibility, duplicate claims, task selection, collusion and total budgets need bounded controls. They restrict promotion funds, not network registration. Do not publish contacts, chat contents or multi-UCA links to qualify for subsidies.

## Treasury and governance

Separate balances, discretionary funds, uncommitted budgets, established commitments and payables. Budgets are allocations of assets, not additional assets. Do not add overlapping reserve/fee/risk aggregates; do not spend customer custody, LP liabilities or personal holdings as treasury cash.

Fund established obligations first, then approved infrastructure, development, promotion and bounded liquidity work. Granted trials, full staged awards, payables and refunds cannot also fund another budget. Treasury recycling is actual approved expenditure, not token burning or fabricated revenue.

The founder retains long-term necessary consent over **new discretionary treasury decisions**, but cannot spend alone. Upgrades, indirect policy changes or replacement authorities cannot bypass that protection. Already established debts settle under their original rules without a new founder veto. Quorum, roles, delay, recovery, succession and deadlock handling remain pending.

Market exchange remains market-driven. Any liquidity intervention has a separate finite authorized budget, not an unlimited buyback or price peg. Report cash, NOV units and reference valuations separately; neither subsidies nor appreciation guarantee provider profitability.

## Affordability and quotes

Service quotes may adjust with cost, quality, capacity and supply/demand, not mechanically track NOV appreciation. Accepted tasks retain their price. If the same resource still costs a reference USD 1, prices of USD 0.05, 1 and 1000 per NOV correspond to 20, 1 and 0.001 NOV. This is arithmetic, not a promised exchange rate or token price.

Proposed protection: a reference-fiat budget alongside a hard NOV authorization cap. Stale/abnormal prices, insufficient qualifying liquidity or cap overruns pause **new tasks relying on that price**, explain the condition and never silently raise authority. Existing funded obligations continue. Sources, validity, conversion, change limits and recovery rules remain pending.

Protocol clearing price, market execution price, UI reference valuation and provider quotes are distinct. Preserve quote/clearing safeguards, permitted sources and epoch rules; AMM spot must not directly determine protocol clearing.

## Scenario arithmetic, not approved spending

- Assumed resource cost `2.05 * 1.4 = USD 2.87`.
- A USD 5 service event yields USD 2.50 at 50% provider share or USD 4.00 at 80%; only the latter exceeds that assumed cost. This does not approve the split or price.
- `5 * (0.8 + 0.1 + 0.1) = 5` demonstrates conservation only.
- `(4000 + 500 + 1800 + 1200) * 1.25 = USD 9,375` is a reference-value scenario, not necessarily that much cash fundraising.
- 80/10/10, USD 5, 5 GiB, 1,000 trial slots and USD 9,375 are not launch parameters or authorized expenditure. Pay-as-you-go revenue cannot be forecast as user count multiplied by USD 5. USD 0.05 or USD 1 issuance-price scenarios do not establish market liquidity/value.

## Reuse, implementation gaps and pending decisions

Reuse existing UCA, resource evidence, [billing contract](NOVONET-SERVICE-BILLING-V1-2026-10-14.md), treasury and settlement ownership; no parallel payment service or ledger.

- The inspected [native Transfer ingress](../../crates/novovm-node/src/native_pipeline/ingress/wire.rs) rejects Execute/Governance. This is a claim about that entry, not every system surface.
- The older [token runtime](../../crates/novovm-consensus/src/token_runtime.rs) uses nine decimal places; this does not prove end-to-end microbilling or provider payouts.
- [Policy](../../crates/novovm-consensus/src/types.rs) and [legacy token implementation](../../vendor/web30-core/src/mainnet_token_impl.rs) retain locked-supply/mint/burn and fee-to-pool behavior that must be reconciled with the fixed-supply target. Setting a field to zero is not acceptance.
- Needed interface behavior: funded reservation, incremental performance reconciliation, concrete payee credit, idempotent release/refund, bounded automatic spending and governance authorization. Billing-to-provider production closure is not delivered.
- Validate supply conservation/unlocks, independent M2 redemption, tiny-value accumulation, duplicate claims, cross-month debts, funded trials, full staged awards, price anomalies and governance debt protection before enabling fees.

Pending: genesis allocation/unlocks, service split/base, actual cash budget/cost evidence, metering/replicas/fanout, trial capacity and airdrop eligibility, billing timezone/DST/cutoff, dual budget rules, governance recovery/succession/quorum, implementation/rounding of the confirmed nine-decimal precision and charging activation. Documentation does not decide these parameters.

## Confirmed resource income and construction funding

This synchronizes Chinese sections 11–12 and the user's approved resource-income plan, incorporating attachment `53e41f68-4464-435e-9140-5b9c5a4bdd99`. **Professional operators, multiple devices/accounts and self-funded tests are allowed. Self-created customer orders do not automatically earn additional public subsidies.** This supersedes the proposal to intentionally leave reward-farming loopholes; professional operation itself is not abuse.

| Income | Funding and acceptance | Insufficient evidence |
| --- | --- | --- |
| Customer service payment | Funded customer budget and verified purchased obligation | Self-reported traffic, ordinary retransmissions, duplicate receipts |
| Construction capacity payment | Finite budget purchases specified region, duration, capacity and availability in advance | Online status, check-ins, device count, claimed free disk |
| Public test payment | Previously published, funded task with defined result requirements | Self-created activity followed by a public subsidy claim |

Public budgets can purchase reserve capacity, coverage and tests before organic demand grows. Each payment identifies its sponsor, purchased work, evidence and unique payable obligation. Contracts state whether capacity fees include usage; included work is not paid twice. Public procurement eligibility is not permission to register a UCA and must not expose contacts or multi-UCA relationships.

A valid receipt proves a stage, not independent parties or public value. Paying `1 NOV`, receiving `0.8` through a controlled node and automatically earning another `0.3` yields `0.1 NOV` before costs. Review service income, subsidies, referrals and other related returns together. Per-UCA caps or a total budget alone do not prove Sybil resistance. Multiple identities are not independent failure domains; capacity cannot be committed or rewarded twice.

“Contribution mining” may describe resource earnings, but not Bitcoin PoW on phones or promised returns. Bitcoin mining competes through consensus work; self-generated transactions do not increase block subsidy ([developer guide](https://developer.bitcoin.org/devguide/mining.html)). NOV construction payments transfer already issued assets.

- Compute verifies purchased tasks/results, not claimed CPU/GPU use; AOEM compute and forwarding have separate meters. L4 service is not validator qualification.
- Custody verifies its contracted interval and retrieval duty; forwarding verifies selected service stages; capacity/discovery/routing verifies purchased availability. No generic contribution score substitutes for these distinct proofs.
- Income states are **estimated → awaiting verification/conditions → established payable → settled spendable NOV**. Conditions and staged release must precede acceptance; earned payables cannot be retroactively converted into locked awards.
- Settled NOV reaches the chosen UCA asset account and has the same asset properties as other spendable NOV. It can be spent/transferred under account authority and sold where a real market exists. Contribution shutdown, uninstall or device replacement does not revoke ownership; no guaranteed USD redemption or livelihood.
- Device service keys submit evidence, not unrestricted spending authority. Automatic spending requires purpose/amount permission; income does not raise caps or fund other UCAs automatically.
- Own local history earns no node payment. Never upload contacts/history/attachments for rewards. Only contracted temporary undelivered ciphertext, authorized target-bound migration packages and necessary purchased capacity qualify. Stop billing after required cleanup; no UCA-only lost-history recovery.
- Disabling contribution stops new work; existing obligations must finish or be validly handed over, not discarded.

## Candidate allocation and corrected cost scenarios

All values below are proposals/reference calculations, not genesis configuration, prices or authorized spending:

- Genesis `35/20/20/10/10/5`: network construction 35m, development treasury 20m, team 20m, sales 10m, liquidity 10m, promotion 5m NOV = 100m. Accounts and control remain pending.
- Spreading 35m over eight years gives 4.375m/year, not mandatory emissions or guaranteed purchasing power. Period caps, carryover and early-use approval remain pending.
- Team candidate: 12-month lock followed by 36 installments. Anchor, first date and beneficiaries remain pending; cumulative integer releases handle residuals. Key recovery does not unlock early.
- Sales inventory is not raised cash; liquidity NOV needs counterpart assets. The same sale proceeds cannot simultaneously fund R&D cash and an invested liquidity position. Six purposes do not prove six independent controllers.
- Candidate service split: 80% direct providers, 10% development treasury, 10% network assurance. A reference gross `.10/GiB` gives `.08` net; a promised `.10` net requires `.125` gross under that split. Settlement/claim fee responsibility must be disclosed. Later assurance spending can also pay providers; 80% is not their lifetime share ceiling. Change future terms only, not accepted obligations.

For two billable forwarding stages, storage and control assumptions, reference USD scenarios are: light `.5×2×.10+1×.001+.025=.126`; normal `5×2×.10+10×.001+.05=1.06`; media-heavy `50×2×.10+100×.001+.10=10.20`. These are not bills. Measure small-task counts, authentication, persistence, receipts, PQ and qualifying privacy paths, with comparable region, units, traffic direction and utilization. Do not double-count the same outbound cost or assume a fixed per-person control cost is proven.

At `.08/GiB` revenue and `.05/GiB` all-in cost, `.03/GiB` profit requires `10,000 GiB` of paid work for reference USD 300: USD 800 revenue and USD 500 cost, about 9.77 TiB. If each service GiB is one effective outbound GiB, a continuous 30-day average is about 33.1 Mbps before overhead/retries/offline time/peaks. This is capacity arithmetic, not calendar billing or guaranteed demand. Re-measure cost with utilization; hardware labels do not promise income.

## Free-trial budget review, not funded slots

Candidate rights: 5 GiB business volume and 30 GiB-days across all approved replicas. At provider reference costs `.08/GiB/billable stage`, `.0008/GiB-day`, plus `.08` control:

| Stages | Provider reference USD cost | Against USD 2 reserved/person |
| --- | ---: | ---: |
| 3 | 1.304 | 0.696 remaining |
| 4 | 1.704 | 0.296 remaining |
| 5 | 2.104 | 0.104 short |

Stage count is not a privacy-hop cap. Do not lower privacy to fit a budget or charge the whole task again on ordinary retry/path replacement; replacement costs belong in the quote/assurance arrangements. Before granting rights, cover worst-case obligations within those rights, funding, capacity and responsible providers. Higher costs require funded coverage or fewer not-yet-awarded slots, never reduced granted rights. `5×3×2=30 GiB-days`, while seven days require 105. A calendar-month trial is not unlimited replica retention; reserve the whole accepted duty, including after the trial ends.

Candidate USD 10,000 pilot: 2,608 three-stage provider cost + 1,392 individual buffer + 3,000 public capacity contracts + 1,000 settlement/conversion/operations + 2,000 general reserve. Combined buffers are 3,392, not two reimbursements for the same replacement. Capacity contracts need region, duration, volume, availability tests and included usage. Cash, funded NOV and reference valuation differ; approved budgets are not deposits.

Candidate batches add `100/400/500/1,000`, cumulatively `100/500/1,000/2,000`. Deduct all outstanding commitments before each batch; temporarily unused rights cannot be promised twice. Full illustrative use means 10,000 GiB business traffic, 30,000 GiB three-stage forwarding and 60,000 GiB-days custody, plus peak/regional validation. This does not establish funded slots or unique humans.

10,000 real paying users at reference USD 1.06 yield 10,600, split 8,480/1,060/1,060 under the candidate policy, not already converted USD. Fund early R&D separately; report sponsored and organic demand separately.

## First verifiable service-income milestone

**Documentation only; implementation starts only after a later explicit development instruction.** Use temporary ciphertext custody/retrieval, existing UCA, native ledger and metering/settlement owners. Required interface behavior, not delivered APIs:

1. Reserve real confirmed funds in the correct chain domain, binding authority, quote, task/deadline and net payee; concurrent requests cannot overspend.
2. Establish payable increments for verified work, not duplicate requests/receipts or initial custody proof of an entire future term.
3. Prove credit to the concrete provider account in confirmed state; reconcile funding, payables and fees, not just pool growth.
4. Spend/transfer that credited balance with proper authority; service keys and incoming earnings cannot expand spending power.
5. Interrupt/restart around reservation, performance, settlement and refund: preserve legitimate payables, no duplicate payment/refund, and resolve late claims consistently.

Use explicitly labeled test NOV on the same product path first. Formal income awaits first-release identity/signature, approved funding rules, new genesis and production acceptance. Construction grants have a separate gate covering duplicate capacity, false independence via multiple UCAs, unauthorized self-created tasks, related returns, duplicate obligations, exhausted budgets and valid handover after shutdown. Proof protocol, task authority, contract/dispute parameters and activation remain pending; this does not prevent the first funded customer-service test closure.
