# NOVORUDP DATA Pacing Signoff

Date: 2026-06-29

This signoff records the 5% DATA loss repair baseline after adding DATA send pacing to the `NovoRudpTransportFrameV0` network-only path.

## Status

```text
Transport Boundary Reset v0: SIGNED
Production Sustained over FrameV0: SIGNED
Legacy Mixed Repair: REMOVED
DATA Loss Repair 5%: SIGNED
DATA Pacing 5% Loss Optimization: PASS / EFFECTIVE
```

## Baseline

Commit: `c6a7cb5`

Result:

```text
DATA loss injected = 130
A repair_sent = 2208
B data_received = 221
B repair_received = 2206
B duplicate_received = 27
B final_missing = 0
B receiver_done = true
```

Meaning:

Transport-native REPAIR recovered all payloads, but primary DATA ingress was too low and repair amplification was high.

## Optimized

Commit: `e031593`

DATA pacing:

```text
NOVOVM_NOVORUDP_NETWORK_ONLY_DATA_PACING_CHUNK_SIZE = 32
NOVOVM_NOVORUDP_NETWORK_ONLY_DATA_PACING_CHUNK_GAP_MS = 5
```

Result:

```text
DATA loss injected = 130
A repair_sent = 130
B data_received = 2270
B repair_received = 130
B duplicate_received = 0
B final_missing = 0
B receiver_done = true
```

## Comparison

```text
B data_received: 221 -> 2270
B repair_received: 2206 -> 130
A repair_sent: 2208 -> 130
B duplicate_received: 27 -> 0
B final_missing: 0 -> 0
```

## Layer Guard

The signed run stayed inside the transport-only layer:

```text
business_payload_mode = opaque
business_decode_count = 0
aoem_executed_total = 0
ledger_completed_count = 0
aoem_execute_enabled = false
legacy mixed path = not used
```

## Conclusion

DATA pacing `32 / 5ms` restores primary DATA ingress close to the theoretical value and reduces repair amplification to the actual injected loss level while preserving `2400 / 2400` transport delivery.

This is the baseline for the next stage:

```text
NOVORUDP Window Control / Throughput Soak
```
