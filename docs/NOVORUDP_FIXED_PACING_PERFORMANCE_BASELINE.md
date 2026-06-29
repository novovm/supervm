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
256 / 1ms
```

The `256 / 1ms` tier remained correct but was slower than `128 / 1ms`, so larger chunks should not be assumed to improve throughput. The observed regression likely comes from local burst shape, ACK cadence, socket drain, or scheduler effects.

## Current Default Recommendation

Use `128 / 1ms` as the current fixed-pacing performance baseline for FrameV0 throughput experiments.

Do not replace it with `256 / 1ms` as a default despite correctness passing.

## Future Experiments

Possible follow-up experiments:

```text
128 / 0ms
192 / 1ms
```

These are exploratory only and must not replace the current baseline unless they pass with:

```text
repair = 0
duplicate = 0
final_missing = 0
ledger_completed = tx_count
receiver_transport_delivery_elapsed_ms < 480
```
