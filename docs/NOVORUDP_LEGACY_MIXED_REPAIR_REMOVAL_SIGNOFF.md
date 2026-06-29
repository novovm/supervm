# NOVORUDP Legacy Mixed Repair Removal Signoff

Date: 2026-06-29

This signoff records the removal of legacy mixed-layer repair attribution after production sustained migrated to `NovoRudpTransportFrameV0`.

## Baseline

Production sustained migration:

- Commit: `d4a78a3`
- Status: `PASS / SIGNED`
- `production_sustained_transport_frame_v0_migration = true`
- `legacy_mixed_repair_used = false`
- `receiver_transport_unique_delivered_count = 2400`
- `receiver_transport_final_missing_count = 0`
- `business_decode_count = 2400`
- `aoem_executed_total = 2400`
- `ledger_completed_count = 2400`

Production fallback removal:

- Commit: `2d214f8`
- Status: `PASS / SIGNED`
- Production sustained is forced onto `NovoRudpTransportFrameV0`.
- The legacy mixed sustained runtime switch can no longer select the production path.
- `legacy_mixed_path_status = removed_from_production_sustained`

## Removal

Legacy repair-like attribution cleanup:

- Commit: `531ef04`
- Status: `PASS / SIGNED`
- Removed mixed-layer repair-like runtime/report attribution from crate code.
- Removed report fields that only existed for `ProtocolMessage::EvmNative::Transactions + transport_auth.frame_kind=repair`.
- Removed legacy repair-like classifier observations from the old native transport path.

The crate tree no longer contains runtime references to:

```text
repair_like
repair-like
frame_kind=repair
```

## Preserved Boundaries

The cleanup intentionally preserves:

- `NovoRudpTransportFrameV0`
- transport-native `DATA / REPAIR / ACK / ENDPOINT / DONE` behavior
- production sustained over the transport-native three-layer path
- boundary/signoff documents as historical records
- `legacy_mixed_path_status = removed_from_production_sustained`

The cleanup intentionally does not preserve:

- the mixed-layer repair runtime path
- repair classification through `EvmNative::Transactions`
- repair-like attribution fields that only served the mixed path
- production fallback to the old mixed sustained path

## Verification

Post-removal checks:

```text
git grep -n "repair_like\|repair-like\|frame_kind=repair" -- crates
cargo fmt --check
cargo check -q -p novovm-node --bins
cargo test -q -p novovm-network -- --test-threads=1
cargo test -q -p novovm-node --bin supervm-novorudp-network-only-gate -- --test-threads=1
```

Expected result:

- no crate matches for legacy repair-like terms,
- formatting passes,
- node binaries check,
- network tests pass,
- transport frame v0 gate tests pass.

Local production sustained smoke:

```text
production_sustained_transport_frame_v0_migration = true
legacy_mixed_repair_used = false
legacy_mixed_path_status = removed_from_production_sustained
receiver_transport_unique_delivered_count = 64
business_decode_count = 64
aoem_executed_total = 64
ledger_completed_count = 64
receiver_transport_final_missing_count = 0
```

## Final Rule

Production sustained proof must use `NovoRudpTransportFrameV0`.

The legacy mixed repair path is historical record only. It must not be used as a runtime fallback, production signoff path, or target for new NOVORUDP behavior fixes.
