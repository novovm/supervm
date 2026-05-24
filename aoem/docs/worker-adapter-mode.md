# Optional Worker Adapter Mode

`aoem-proof-worker` is a reference host adapter for teams that want to try AOEM
through JSONL jobs before embedding the dynamic library directly.

It is not the AOEM runtime itself, not a standalone platform service, and not
required for production hosts that call AOEM directly.

## Windows

```powershell
aoem\bin\windows-x86_64\aoem-proof-worker.exe `
  --library aoem\windows\core\bin\aoem_ffi.dll `
  --input aoem\worker-adapter\examples\jobs.zk_merkle.jsonl `
  --output tmp\proofs.zk_merkle.jsonl `
  --batch-count 4
```

## Linux

```bash
LD_LIBRARY_PATH=aoem/linux/core/bin \
  aoem/bin/linux-x86_64/aoem-proof-worker \
  --library aoem/linux/core/bin/libaoem_ffi.so \
  --input aoem/worker-adapter/examples/jobs.zk_merkle.jsonl \
  --output /tmp/proofs.zk_merkle.jsonl \
  --batch-count 4
```

## macOS

```text
macOS worker binary and runtime library are pending fresh FULLMAX rebuild and
are not bundled in this SUPERVM package.
```

## Example Job Files

```text
worker-adapter/examples/jobs.merkle.jsonl
  public Merkle inclusion jobs plus one malformed rejection case

worker-adapter/examples/jobs.zk_merkle.jsonl
  private ZK Merkle membership jobs plus one malformed rejection case

worker-adapter/examples/jobs.mixed.jsonl
  mixed public/private profile jobs for host adapter trials
```

## Expected Summary

```text
AOEM_PROOF_WORKER_SUMMARY|profile=zk_merkle_membership_v1|jobs=4|batch_count=4|resident_asset=ok|privacy=ok|proof=ok|verify=ok|external_verify=ok|malformed=ok|failures=0
```

The worker adapter writes JSONL results. Malformed jobs must fail
deterministically with:

```json
{"status":"error","error":"malformed_payload","proof_written":false}
```

## Boundary

```text
optional adapter only
not a required AOEM service
not a performance-ready claim
no new public FFI ABI
no Runtime Canon change
no new compute op
macOS runtime availability is not claimed by this package
```
