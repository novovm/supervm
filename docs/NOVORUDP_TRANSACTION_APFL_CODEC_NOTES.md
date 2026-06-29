# NOVORUDP Transaction APFL Codec Notes

Date: 2026-06-30

Status: `LEARNING / ARCHITECTURE NOTES`

This document records the current understanding for using APFL-style structured compression in the NOVORUDP batch payload path.

It is not a behavior signoff and not an implementation change.

## Current Baseline

NOVORUDP v0 is already separated into:

```text
NOVORUDP transport
  -> opaque payload delivery
business payload decode
  -> evm_transactions decode
AOEM / ledger
  -> explicit execution after decode
```

Signed baseline:

```text
Transport Boundary Reset v0: SIGNED
Production Sustained over FrameV0: SIGNED
Legacy Mixed Repair: REMOVED
DATA Loss Repair 5%: SIGNED
Fixed Pacing Recommended Baseline: 128 / 1ms
```

Batch payload v0 correctness:

```text
payload_count = 4800
txs_per_payload = 128
business_transactions = 614400
transport delivered = 4800
business decoded = 614400
AOEM executed = 614400
ledger completed = 614400
repair = 0
duplicate = 0
final_missing = 0
```

Observed throughput bottleneck:

```text
receiver_payload_bytes_total = 151607870
bytes_per_payload ~= 31.6 KB
bytes_per_tx ~= 247 B
receiver_transport_delivery_elapsed_ms = 32450
ledger_transactions_per_sec ~= 18933
```

Conclusion:

```text
Batch correctness passed.
Throughput is not final.
The current bottleneck is expanded transaction bytes plus encode/copy/send/decode/execute cost.
```

## Boundary Rule

APFL must not be placed inside NOVORUDP transport.

NOVORUDP must continue to treat payloads as opaque bytes:

```text
DATA / REPAIR / ACK remain transport-native.
ACK and REPAIR operate on transport object delivery only.
NOVORUDP does not understand EVM, AOEM, ledger, APFL formulas, or transaction semantics.
```

The APFL transaction codec belongs here:

```text
business payload codec layer
```

Expected layering:

```text
NOVORUDP TransportFrameV0
  carries opaque compact business payload bytes

Transaction APFL Codec
  decodes compact batch payload into materialized or structural transaction batch

AOEM Execution
  executes decoded transaction batch and closes ledger state
```

## APFL Idea To Reuse

The relevant APFL principle is not generic compression.

The relevant principle is:

```text
Find invariants.
Find formulas.
Transmit compact parameters, deltas, and residuals.
Reconstruct materialized payload at the receiver.
```

For transaction batches, this maps to:

```text
TransactionInvariantBank
TransactionFormulaBank
BatchTemplate
DeltaStream
ResidualStream
SignatureStream
Commitment
```

The payload should move from:

```text
tx1 full bytes
tx2 full bytes
tx3 full bytes
...
```

Toward:

```text
batch_template_id
shared_fields
formula_id
delta_params
residuals
signature_refs or signature_payload
commitment
```

## Candidate Transaction Invariants

Likely repeated or structured fields:

```text
chain_id
transaction type
gas policy
fee policy
to address set
method selector
ABI layout
calldata prefix
account set
nonce sequence
value distribution
signature shape
```

Likely delta or formula fields:

```text
nonce delta
amount delta
gas delta
calldata suffix delta
recipient index
method argument delta
timestamp or slot delta
```

Likely residual fields:

```text
unstructured calldata bytes
unique signature bytes
unexpected address bytes
outlier gas or value bytes
```

## What Must Be Measured First

Before implementing a compact codec, the current expanded payload must be attributed.

Minimum sender-side attribution:

```text
sender_payload_bytes_total
sender_payload_bytes_per_payload_min
sender_payload_bytes_per_payload_p50
sender_payload_bytes_per_payload_p95
sender_payload_bytes_per_payload_max
sender_payload_bytes_per_tx
sender_batch_build_elapsed_ms
sender_business_encode_elapsed_ms
sender_payload_copy_elapsed_ms
sender_socket_send_elapsed_ms
```

Minimum receiver-side attribution:

```text
receiver_payload_bytes_total
receiver_payload_bytes_per_payload_min
receiver_payload_bytes_per_payload_p50
receiver_payload_bytes_per_payload_p95
receiver_payload_bytes_per_payload_max
receiver_payload_bytes_per_tx
receiver_business_decode_elapsed_ms
receiver_business_decode_per_tx_ns
receiver_aoem_execute_elapsed_ms
receiver_aoem_execute_per_tx_ns
receiver_ledger_close_elapsed_ms
transport_payloads_per_sec
transport_bytes_per_sec
business_transactions_per_sec
ledger_transactions_per_sec
```

Questions to answer:

```text
1. Is sender time dominated by batch build, business encode, payload copy, or socket send?
2. Is receiver time dominated by business decode, AOEM execution, or ledger close?
3. Which transaction fields dominate bytes_per_tx?
4. Which repeated fields can become invariants?
5. Which variable fields can become delta streams?
6. Which bytes are true residuals that must remain raw?
```

## Implementation Direction Later

The next implementation phase should not blindly increase:

```text
txs_per_payload = 256 / 512 / 1024
```

If each transaction remains fully expanded, larger batches mostly increase payload size and copy cost.

The better direction is:

```text
AOEM Transaction APFL Codec v0
```

Goal:

```text
Reduce bytes_per_tx before increasing tx density further.
```

Example target progression:

```text
247 B / tx  -> current expanded baseline
50 B / tx   -> useful compact batch target
20 B / tx   -> strong structured batch target
```

## Non-Goals

Do not:

```text
change NOVORUDP transport ABI for APFL
make ACK or REPAIR understand APFL or transactions
put formula banks into transport state
use AOEM ledger close as transport delivery state
reintroduce legacy mixed repair through business envelopes
claim million TPS from payload/s alone
```

## Current Decision

The next safe step is:

```text
NOVORUDP Batch Payload Size / Serialization Attribution
```

This is an observation step.

It should only add report fields and timing attribution. It should not change transport behavior, ACK/REPAIR behavior, fixed pacing, or AOEM semantics.

After that attribution exists, the codec can be designed from measured byte and time costs instead of assumptions.

## Algebraic Transaction Batch Direction

The next structural direction is not generic compression.

It is:

```text
NOVORUDP Algebraic Transaction Batch
```

The target transformation is:

```text
128 complete transaction wire objects
```

into:

```text
1 shared transaction structure template
+ N columns of minimal per-transaction parameters
+ proof or digest material needed to reconstruct canonical transactions
```

For highly regular native transfer traffic, repeated fields should not be transmitted for every transaction:

```text
chain_id
tx_type
method
asset_id
fee_policy
receiver route prefix
signature scheme
wire version
execution template
```

The compact payload should look more like:

```text
BatchHeader:
  chain_id
  wire_version
  tx_family
  asset_id
  method_id
  shared_fee_policy
  shared_template_hash

PerTxColumns:
  nonce_delta[]
  amount_delta[]
  to_suffix[]
  signature[]
  optional_memo_digest[]
```

This is the transaction-layer form of APFL:

```text
shared invariant bank
+ formula or template id
+ delta streams
+ residual streams
```

## Semantic Safety Constraint

The algebraic batch codec must not weaken transaction semantics.

Even if the network payload is compact and structured, the receiver must be able to reconstruct:

```text
canonical_tx_i
```

Then the normal execution path must still be able to:

```text
verify signature
compute tx_hash
write ledger
generate receipt
replay deterministically
```

Hard rules:

```text
network representation may become structural
execution semantics must not shrink
receipt semantics must not change
canonical replay must remain possible
transaction hash rules must remain explicit and testable
```

## First Codec Family

The first implementation family should be intentionally narrow:

```text
native_transfer_batch_v0
```

Initial coverage:

```text
same chain
same transaction type
same asset
same sender or sender-prefix family
same fee policy
same signature scheme
native transfer only
```

Explicitly out of scope for the first family:

```text
general EVM contract calls
multi-method batches
multi-asset batches
arbitrary ABI payloads
signature aggregation
session-key authorization changes
consensus or ledger semantic changes
```

Reason:

```text
Start with the most regular transaction family.
Prove byte reduction and canonical reconstruction before generalizing.
```

## Byte Targets

Current expanded baseline:

```text
~247 B / tx
```

First practical target:

```text
80-120 B / tx
```

This target is realistic if each transaction still carries an individual signature.

Expected remaining cost shape:

```text
signature ~= 64 B / tx
receiver / amount / nonce ~= 16-32 B / tx
batch overhead amortized across the payload
```

More aggressive targets:

```text
30-60 B / tx
```

Those likely require additional security-model work such as:

```text
account-family authorization
session keys
signature aggregation
batch authorization
```

Those are not part of v0.

## Single Payload APFL Density Rule

APFL should be applied inside each batch payload, not only across payloads.

The important unit is:

```text
1 NOVORUDP payload = one APFL-encoded transaction batch
```

The incorrect high-throughput model is:

```text
1 payload = 1 fully expanded transaction
```

The correct high-throughput model is:

```text
1 payload = N transactions represented as shared algebraic structure
```

Expanded batch payload:

```text
tx1 full wire
tx2 full wire
tx3 full wire
...
txN full wire
```

APFL batch payload:

```text
shared_header
chain_id
tx_family / template_id
nonce_base + nonce_delta[]
value_or_amount_column[]
receiver_column[]
signature_stream[]
residual_stream[]
commitment
```

This matters because the current bottleneck is not only payload count. It is:

```text
payload size
serialization
copy
socket write
receiver decode
AOEM / ledger per-tx materialization
```

Reducing `bytes_per_tx` directly reduces:

```text
sender byte pressure
socket write pressure
receiver decode/copy pressure
cache misses
batch AOEM execution cost
```

For v0, do not remove signatures:

```text
keep original per-transaction signature field
do not perform signature aggregation
do not change authorization semantics
```

The compression comes from:

```text
shared fields
repeated structure
nonce delta
common gas / fee / method params
address or account columns
amount / value columns
template id
reduced repeated envelope / length / type metadata
```

Therefore the next million-TPS path is not more single-payload pacing. It is:

```text
stable payload/s
* higher tx density per payload
* lower bytes_per_tx inside each payload
```

## Future Implementation Target

When implementation starts, the first real task should be:

```text
feat(novorudp): add native transfer algebraic batch wire v0
```

Required shape:

```text
1. Add NativeTransferBatchV0 encoding structure.
2. Store shared fields in a batch header.
3. Store per-transaction fields as columns.
4. Reconstruct canonical transactions on the receiver.
5. Verify reconstructed tx_hash against the legacy native tx hash rule.
6. Keep ledger and receipt semantics unchanged.
7. Keep NOVORUDP transport unchanged.
8. Keep the codec behind an explicit mode or test-only gate until signed.
```

Required report fields:

```text
legacy_bytes_per_tx
algebraic_batch_bytes_per_tx
algebraic_batch_bytes_total
algebraic_batch_savings_ratio_bps
canonical_reconstruction_count
canonical_reconstruction_error_count
canonical_tx_hash_match_count
canonical_tx_hash_mismatch_count
```

The first success condition is not million TPS.

The first success condition is:

```text
native_transfer_batch_v0 reconstructs canonical transactions exactly
and reduces bytes_per_tx versus the expanded batch baseline.
```
