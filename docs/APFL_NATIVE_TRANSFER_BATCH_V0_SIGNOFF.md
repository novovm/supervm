# APFL NativeTransferBatchV0 Signoff

Date: 2026-06-30

Status: `PASS / SIGNED`

Code baseline:

```text
SUPERVM HEAD = 0169732
Implementation commit = 08ad5d8
Payload mode = native_transfer_apfl_v0
```

## Scope

This signoff covers the first APFL transaction codec family:

```text
native_transfer_batch_v0
```

Confirmed constraints:

```text
native transfer family only
no generic EVM contract call codec
per-transaction original signature field preserved
no signature aggregation
canonical hash uses current NOVOVM native tx hash rule
receiver reconstructs canonical native tx before existing AOEM adapter execution
NOVORUDP transport ABI unchanged
```

Layering:

```text
NOVORUDP TransportFrameV0
  -> opaque APFL compact payload bytes
  -> canonical native tx reconstruction
  -> existing AOEM adapter
  -> ledger close
```

## Small Cross-Machine Sample

Run:

```text
real-ab-apfl-native-transfer-480x128-20260630-0169732-071607
```

Parameters:

```text
payload_count = 480
txs_per_payload = 128
total_tx = 61440
execute_aoem = 1
pacing = 128 / 1ms
```

Result:

```text
accepted = true
transport_payloads_delivered = 480
receiver_transport_final_missing_count = 0
receiver_transport_repair_received_count = 0
receiver_transport_duplicate_received_count = 0

business_transactions_decoded_count = 61440
aoem_transactions_executed_total = 61440
ledger_transactions_completed_count = 61440

apfl_binary_bytes_per_tx = 32
legacy_bytes_per_tx = 238
apfl_binary_savings_ratio_bps = 8626

canonical_tx_hash_match_count = 61440
canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0

receiver_transport_delivery_elapsed_ms = 389
business_transactions_per_sec = 157943
ledger_transactions_per_sec = 157943
```

Classification:

```text
PASS
```

## Large Cross-Machine Sample

Run:

```text
real-ab-apfl-native-transfer-4800x128-20260630-0169732-072204
```

Parameters:

```text
payload_count = 4800
txs_per_payload = 128
total_tx = 614400
execute_aoem = 1
pacing = 128 / 1ms
```

Result:

```text
accepted = true
transport_payloads_delivered = 4800
receiver_transport_final_missing_count = 0
receiver_transport_repair_received_count = 16
receiver_transport_duplicate_received_count = 0

business_transactions_decoded_count = 614400
aoem_transactions_executed_total = 614400
ledger_transactions_completed_count = 614400

apfl_binary_bytes_per_tx = 32
legacy_bytes_per_tx = 242
apfl_binary_savings_ratio_bps = 8650

canonical_tx_hash_match_count = 614400
canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0

receiver_transport_delivery_elapsed_ms = 4332
business_transactions_per_sec = 141828
ledger_transactions_per_sec = 141828
```

Classification:

```text
PASS / SIGNED
```

The small repair count:

```text
repair_received = 16 / 4800
```

is not a correctness blocker:

```text
final_missing = 0
duplicate = 0
ledger completed = 614400
```

It should be tracked as a later transport tuning item, not as an APFL codec failure.

## Compression Result

Large sample byte reduction:

```text
legacy_bytes_per_tx = 242
apfl_binary_bytes_per_tx = 32
compression ~= 7.56x
savings = 8650 bps
```

The first-stage target was:

```text
bytes_per_tx <= 120 B
```

Observed result:

```text
32 B / tx
```

Therefore the first APFL compact transaction representation target is exceeded.

## Correctness Result

The following held at 614400 tx scale:

```text
canonical_tx_hash_match_count = 614400
canonical_tx_hash_mismatch_count = 0
signature_verify_error_count = 0
business_transactions_decoded_count = 614400
aoem_transactions_executed_total = 614400
ledger_transactions_completed_count = 614400
```

Conclusion:

```text
APFL compact payload can cross machines over NOVORUDP,
reconstruct canonical native transactions,
preserve hash/signature semantics,
and close AOEM/ledger through the existing adapter path.
```

## Signed State

```text
APFL NativeTransferBatchV0 = CROSS-MACHINE LARGE SAMPLE PASS
614400 tx = SIGNED
32 B / tx compact payload = VALIDATED
canonical reconstruction = VALIDATED
AOEM / ledger close = VALIDATED
NOVORUDP transport ABI = unchanged
```

## Throughput Interpretation

This signoff is not a million TPS signoff.

Observed large-sample throughput:

```text
614400 tx / 4.332s ~= 141828 TPS
```

Previous expanded batch baseline:

```text
~18933 TPS
```

Observed throughput improvement:

```text
141828 / 18933 ~= 7.49x
```

This closely matches the byte reduction:

```text
legacy_bytes_per_tx = 242 B
apfl_binary_bytes_per_tx = 32 B
242 / 32 ~= 7.56x
```

Conclusion:

```text
APFL is effective.
Throughput improvement is currently dominated by byte density improvement.
```

The remaining gap to one million TPS is not a correctness issue and not a sign that APFL failed.

At `32 B / tx`, one million TPS requires at least:

```text
~32 MB/s effective transaction bytes throughput
```

The signed large sample achieved approximately:

```text
32 B/tx * 614400 tx / 4.332s ~= 4.5 MB/s
```

Therefore the next bottleneck moved to:

```text
effective bytes/sec
sender encode / copy / socket send
receiver APFL decode
canonical reconstruction
AOEM adapter_projection_v0 execution
ledger close
debug vs release runtime
single lane / single socket limits
```

The next optimization stage should measure these costs before changing APFL semantics.

## Next Work

Do not broaden to generic EVM calls immediately.

Recommended next steps:

```text
1. Record this as the native transfer APFL baseline.
2. Tune the low repair count later if needed.
3. Add sender/receiver timing attribution for encode/copy/socket/decode/reconstruction/AOEM/ledger.
4. Measure release-mode throughput.
5. Explore zero-copy columnar view only after timing attribution identifies copy/decode as bottlenecks.
6. Explore multi-lane / multi-socket only after single-lane release baseline is measured.
7. Only then consider broader transaction families.
```
