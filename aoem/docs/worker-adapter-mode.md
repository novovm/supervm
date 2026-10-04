# Optional Worker Adapter Mode

`aoem-proof-worker` is a reference host adapter for teams that want to try AOEM
through JSONL jobs before embedding the dynamic library directly.

It is not the AOEM runtime itself, not a standalone platform service, and not
required for production hosts that call AOEM directly.

Profiles 1/2 provide public diagnostics only (`scope=not_zk`, wire/JSON scope
`envelope_integrity_only_not_zk`). Profile 3 (`zk_merkle_membership_v1`) is
retired: its public envelope did not prove the private relation. It must return
an error, never a successful private proof. See the
[security correction](proof-engine-v1.0-contract.md#security-correction-2026-10-04).

## Windows

```powershell
New-Item -ItemType Directory -Force tmp | Out-Null
aoem\bin\windows-x86_64\aoem-proof-worker.exe `
  --library aoem\windows\core\bin\aoem_ffi.dll `
  --input aoem\worker-adapter\examples\jobs.merkle.jsonl `
  --output tmp\public-diagnostics.jsonl `
  --batch-count 4
```

## Linux

```bash
LD_LIBRARY_PATH=aoem/linux/core/bin \
  aoem/bin/linux-x86_64/aoem-proof-worker \
  --library aoem/linux/core/bin/libaoem_ffi.so \
  --input aoem/worker-adapter/examples/jobs.merkle.jsonl \
  --output /tmp/public-diagnostics.jsonl \
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
  public path/envelope diagnostics plus one malformed rejection case

worker-adapter/examples/jobs.zk_merkle.jsonl
  retired-profile rejection fixtures; no successful private proof output

worker-adapter/examples/jobs.mixed.jsonl
  mixed diagnostics/rejections; private jobs remain errors
```

## Expected Diagnostic Result

Every successful public result must contain these exact values:

```json
{
  "status": "ok",
  "profile_id": "merkle_membership_v1",
  "verify_status": "envelope_integrity_only_not_zk",
  "accepted": false,
  "proof_verified": false,
  "envelope_integrity_verified": true,
  "cryptographic_proof_verified": false,
  "verification_scope": "envelope_integrity_only_not_zk"
}
```

These are selected result fields, not a complete schema example. A zero worker
exit code or the historical summary labels `proof=ok` / `verify=ok` only indicate
that the public diagnostic ran; they do not certify ZK or business execution.

To exercise retirement, use `jobs.zk_merkle.jsonl` instead: expect exit code 1
and `status=error`, `error=unsupported_private_membership_proof`,
`proof_written=false`. Do not treat that expected rejection as a private-proof
success or replace it with a witness-disclosure fallback.

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
