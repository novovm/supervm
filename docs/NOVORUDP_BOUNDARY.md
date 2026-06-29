# NOVORUDP Transport Boundary

This document is a hard boundary rule for NOVORUDP work.

## Current Status

The current sustained repair path is frozen as a mixed-layer legacy path. It proved useful for attribution, but it must not be extended as the target architecture.

Known legacy coupling:

- Repair traffic is currently represented as `ProtocolMessage::EvmNative::Transactions`.
- Repair classification is currently inferred from `transport_auth.frame_kind=repair`.
- Some ACK/missing/repair observations are tied to AOEM ledger progress.

These facts are compatibility debt, not the NOVORUDP design.

## Boundary Rules

NOVORUDP must be an independent transport layer.

- Transport frames must not depend on `EvmNative::Transactions`.
- Repair must not be encoded as a business transaction frame.
- Business payloads must not participate in repair classification.
- Transport ACK must confirm packet/object delivery only.
- Transport ACK must not depend on AOEM ledger close.
- AOEM may consume delivered payloads, but AOEM must not drive transport receive accounting.
- Receiver networking must drain UDP packets into a transport queue before business decode or AOEM execution.
- Transport delivery must complete before the payload enters the business/native pipeline.

## Required v0 Reset

The next target architecture is `NOVORUDP Transport Boundary Reset v0`.

Required transport frame kinds:

- `DATA`
- `REPAIR`
- `ACK`
- `ENDPOINT`
- `DONE`

`DATA` and `REPAIR` carry opaque business payload bytes. Business payload decode happens only after transport delivery.

## Validation Gates

Sustained validation must be split into three gates:

1. Network-only delivery sustained.
   - DATA/REPAIR/ACK close independently.
   - All payload objects are transport-delivered.
   - Missing, duplicate, repair, and coverage are reported by transport state.
   - AOEM is not part of this gate.

2. Business payload delivery sustained.
   - Transport-delivered opaque payloads decode into business messages.
   - AOEM ledger close is not required for this gate.

3. AOEM ledger sustained.
   - Delivered business payloads enter AOEM.
   - Ledger completion and receiver_done are validated here.

## Legacy Compatibility Rule

Any remaining use of `ProtocolMessage::EvmNative::Transactions` plus `transport_auth.frame_kind=repair` must be treated as legacy compatibility or attribution only.

New behavior fixes must not deepen this mixed-layer path. If a change needs transport repair behavior, it must target explicit NOVORUDP transport frames or a clearly marked boundary-reset implementation.
