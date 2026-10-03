# Preserved components awaiting product integration

These are the post-isolation implementations, retained with their tests and
proof evidence. The actual product controller is `../crates/novovm-node`.
This separate workspace prevents a name collision between the two existing
`novovm-network` libraries; it does not introduce a second default product.

From the repository root, use explicit manifests:

```powershell
pwsh -File runtime/check-isolation.ps1
cargo test --manifest-path runtime/Cargo.toml --workspace --locked
cargo clippy --manifest-path runtime/Cargo.toml --workspace --all-targets --locked -- -D warnings
```

Real AOEM component tests require the actual platform SDK binary, not an LFS
pointer. Before launching, `AOEM_PERSISTENCE_PATH` must not be nonempty and
`AOEM_BENCH_RELAXED_SYNC` must be absent, including an empty value. From the root:

```powershell
$library = if ($IsWindows) { 'aoem/windows/core/bin/aoem_ffi.dll' } else { 'aoem/linux/core/bin/libaoem_ffi.so' }
$env:NOVOVM_AOEM_TEST_LIBRARY = (Resolve-Path $library).Path
cargo test --manifest-path runtime/Cargo.toml --workspace --release --locked -- --include-ignored --test-threads=1
```

The independent `proofs/nov-transfer` workspace and its lock files are retained.
Do not infer node integration, mainnet throughput, privacy or PQ acceptance
from these component checks. Preserve the historical results and failures in
the [readiness tracker](../docs/NOVOVM_PRODUCTION_READINESS_TRACKER.md); integrate
reviewed components via the original unified `novovm-exec` facade.
