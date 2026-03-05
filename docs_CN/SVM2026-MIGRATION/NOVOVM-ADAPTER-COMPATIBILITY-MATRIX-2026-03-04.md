# NOVOVM Adapter Compatibility Matrix - 2026-03-04

## Scope

- migration item: `F-08`
- source of truth: `config/novovm-adapter-compatibility-matrix.json`
- plugin registry: `config/novovm-adapter-plugin-registry.json`

## Supported Matrix (Current)

| Backend | Crate | Chains | ABI | Required Caps |
|---|---|---|---:|---:|
| native | `novovm-adapter-novovm` | `novovm`, `evm`, `bnb`, `custom` | n/a | n/a |
| plugin | `novovm-adapter-sample-plugin` | `novovm`, `evm`, `bnb`, `custom` | 1 | `0x1` |

## Validation Commands

```powershell
cargo test --manifest-path crates/novovm-adapter-novovm/Cargo.toml
cargo test --manifest-path crates/novovm-adapter-sample-plugin/Cargo.toml
cargo test --manifest-path crates/novovm-node/Cargo.toml
```

## Notes

- Non-`novovm/custom` samples are migration-oriented compatibility stubs for `evm`/`bnb`.
- Production chain-specific semantics are still expected to be implemented by dedicated adapters in later phases.

