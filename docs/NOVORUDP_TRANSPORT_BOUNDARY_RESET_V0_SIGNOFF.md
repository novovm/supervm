# NOVORUDP Transport Boundary Reset v0 Signoff

Date: 2026-06-29

This signoff records the validated NOVORUDP v0 path after separating transport, business payload decode, and AOEM/ledger projection.

## Signed Layers

### 1. Transport Layer

Commit: `78f9319`

Status: `PASS / SIGNED`

Result:

- `NovoRudpTransportFrameV0` delivered 2400 opaque payloads cross-machine.
- `receiver_transport_unique_delivered_count = 2400`
- `receiver_transport_final_missing_count = 0`
- `receiver_transport_done = true`
- `business_decode_count = 0`
- `aoem_executed_total = 0`
- `ledger_completed_count = 0`

Meaning:

NOVORUDP transport-native frames can reliably deliver opaque bytes without depending on business payloads or AOEM ledger state.

### 2. Business Payload Decode Layer

Commit: `6732533`

Status: `PASS / SIGNED`

Result:

- `business_payload_mode = evm_transactions`
- `receiver_transport_unique_delivered_count = 2400`
- `receiver_transport_final_missing_count = 0`
- `business_decode_count = 2400`
- `business_decode_error_count = 0`
- `aoem_executed_total = 0`
- `ledger_completed_count = 0`

Meaning:

Business decode happens only after transport delivery. NOVORUDP still treats payloads as opaque bytes.

### 3. AOEM / Ledger Adapter Projection

Commit: `4ed6a62`

Status: `PASS / SIGNED`

Result:

- `receiver_transport_unique_delivered_count = 2400`
- `receiver_transport_final_missing_count = 0`
- `business_payload_mode = evm_transactions`
- `business_decode_count = 2400`
- `business_decode_error_count = 0`
- `aoem_execution_mode = adapter_projection_v0`
- `aoem_executed_total = 2400`
- `aoem_execution_error_count = 0`
- `ledger_completed_count = 2400`

Meaning:

AOEM/ledger projection enters only after transport delivery and business payload decode. This preserves the boundary:

```text
NOVORUDP transport -> business payload decode -> AOEM/ledger projection
```

## Legacy Mixed Path Status

The previous sustained path using:

```text
ProtocolMessage::EvmNative::Transactions + transport_auth.frame_kind=repair
```

is frozen as legacy mixed-layer attribution/compatibility only.

It must not be used as the target architecture for new NOVORUDP behavior fixes because it couples:

- transport repair
- transport ACK/missing state
- business transaction envelope
- AOEM/ledger close

## Forward Rule

New NOVORUDP work should extend the transport-native path only.

Allowed:

- `NovoRudpTransportFrameV0` improvements
- transport-level pacing, repair, ACK, duplicate suppression, and receive drain
- payload decode after transport delivery
- AOEM execution after payload decode

Not allowed as target architecture:

- classifying transport repair through business transaction messages
- using AOEM/ledger progress as transport receive accounting
- adding behavior fixes to deepen the legacy mixed repair path

## Next Migration Decision

The next engineering decision is whether to:

- retire the legacy mixed sustained path,
- keep it only behind a compatibility flag,
- or migrate production sustained execution onto `NovoRudpTransportFrameV0`.

