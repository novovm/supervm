# NOVORUDP Fixed Pacing Performance Baseline

Date: 2026-06-30

This document records the fixed DATA pacing parameter sweep for `NovoRudpTransportFrameV0`.

## Scope

The sweep used the existing FrameV0 network-only runner with:

```text
tx_count = 4800
payload_mode = evm_transactions
execute_aoem = 1
loss = disabled
```

No code behavior was changed during the sweep. Only runtime pacing parameters changed.

## Signed Baseline

Baseline commit:

```text
9f78082
```

Functional requirements held for all clean tiers:

```text
receiver_transport_unique_delivered_count = 4800
receiver_transport_final_missing_count = 0
receiver_transport_repair_received_count = 0
receiver_transport_duplicate_received_count = 0
business_decode_count = 4800
aoem_executed_total = 4800
ledger_completed_count = 4800
```

## Results

```text
32 / 5ms:
  receiver_transport_delivery_elapsed_ms = 879
  approximate delivery rate = 5,461 payload/s
  sender_data_frames_per_sec = 5,111

64 / 2ms:
  receiver_transport_delivery_elapsed_ms = 616
  approximate delivery rate = 7,792 payload/s
  sender_data_frames_per_sec = 7,164

128 / 1ms:
  receiver_transport_delivery_elapsed_ms = 480
  approximate delivery rate = 10,000 payload/s
  sender_data_frames_per_sec = 8,988

128 / 0ms:
  receiver_transport_delivery_elapsed_ms = 617
  approximate delivery rate = 7,779 payload/s
  sender_data_frames_per_sec = 7,111
  note = clean but slower; gap 0ms disables pacing in the current implementation

192 / 1ms:
  receiver_transport_delivery_elapsed_ms = 613
  approximate delivery rate = 7,830 payload/s
  sender_data_frames_per_sec = 7,121
  note = clean but slower

256 / 1ms:
  receiver_transport_delivery_elapsed_ms = 638
  approximate delivery rate = 7,524 payload/s
  sender_data_frames_per_sec = 6,818
```

## Decision

Current best fixed pacing tier:

```text
128 / 1ms
```

Clean but regressive tier:

```text
128 / 0ms
192 / 1ms
256 / 1ms
```

The `128 / 0ms`, `192 / 1ms`, and `256 / 1ms` tiers remained correct but were slower than `128 / 1ms`, so larger chunks or removing the gap should not be assumed to improve throughput. The observed regression likely comes from local burst shape, ACK cadence, socket drain, or scheduler effects.

Important current implementation detail:

```text
chunk_gap_ms = 0
=> sender_transport_data_pacing_enabled = false
=> this behaves as pacing disabled / burst-like send
```

Therefore `128 / 0ms` is not recommended as a default even though it is clean.

## Current Default Recommendation

Use `128 / 1ms` as the current fixed-pacing performance baseline for FrameV0 throughput experiments.

Do not replace it with `256 / 1ms` as a default despite correctness passing.

## Future Experiments

Single-payload fixed pacing has a clear current best tier. Future throughput work should move to batch payload density instead of continuing to tune only payload/s.

```text
NOVORUDP Batch Payload Throughput v0
```

The next stage should measure:

```text
transport_payloads_per_sec
txs_per_payload
business_transactions_per_sec
aoem_batches_executed
aoem_transactions_executed
ledger_transactions_completed
```
