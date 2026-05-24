# SUPERVM AOEM FULLMAX Proof Quickstart

This quickstart runs the in-place SUPERVM AOEM host package under `aoem/`.
It currently includes freshly rebuilt Windows and Linux FULLMAX artifacts.
macOS remains pending.

## Product Path

```text
host / optional worker adapter
  -> aoem_execute_ops_wire_v1
  -> compute.zk.resident_proof_v1
  -> profile_id selects proof semantics
  -> AOEM state
  -> aoem_state_read_v1
```

## Windows Embedded Smoke

```powershell
powershell -ExecutionPolicy Bypass -File scripts\aoem\run_proof_engine_host_smoke.ps1 -SkipWorkerAdapter
```

Expected output:

```text
SUPERVM_AOEM_PROOF_ENGINE_HOST_SMOKE|profile=fixed_profile_v1|proof=ok|verify=ok|state_read=ok|metadata=ok|failures=0
```

## Windows Worker Adapter

```powershell
aoem\bin\windows-x86_64\aoem-proof-worker.exe `
  --library aoem\windows\core\bin\aoem_ffi.dll `
  --input aoem\worker-adapter\examples\jobs.zk_merkle.jsonl `
  --output tmp\proofs.zk_merkle.jsonl `
  --batch-count 4
```

The worker adapter is optional. Production SUPERVM hosts can embed the AOEM
dynamic library directly.

## Linux Worker Adapter

```bash
LD_LIBRARY_PATH=aoem/linux/core/bin \
  aoem/bin/linux-x86_64/aoem-proof-worker \
  --library aoem/linux/core/bin/libaoem_ffi.so \
  --input aoem/worker-adapter/examples/jobs.zk_merkle.jsonl \
  --output /tmp/proofs.zk_merkle.jsonl \
  --batch-count 4
```

## Schemas

```text
schemas/proof_job.schema.json
schemas/proof_result.schema.json
schemas/proof_profiles.json
```

Worker output currently encodes `public_outputs` and `metadata` as JSON strings
because those fields are copied from AOEM state readback responses.

## Platform Boundary

```text
Windows runtime = included
Linux runtime   = included
macOS runtime   = pending_rebuild_not_bundled
```

Old macOS dynamic libraries were removed from the SUPERVM package. Do not claim
macOS runtime availability until a fresh FULLMAX artifact is materialized and
verified.

## Non-Claims

```text
not a generic arbitrary-circuit proof system
not a performance-ready claim
not a Graph OS path
not a dedicated LR path
no new public FFI ABI
no Runtime Canon change
no new compute op
```
