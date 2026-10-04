# Worker Adapter Reference

`aoem_proof_worker.c` is an optional reference host adapter. It reads JSONL
jobs, calls the AOEM dynamic library through `aoem_execute_ops_wire_v1`, reads
diagnostic outputs through `aoem_state_read_v1`, and writes JSONL results.
It includes the single corrected source in `../examples/aoem_proof_worker.c`.

This adapter is useful for SDK trials, CI acceptance, and sidecar-style
integration while a host team migrates to direct embedded use.

It is not the AOEM runtime itself and not the required production deployment
model.

Profiles 1/2 are `scope=not_zk`: successful rows have
`verification_scope=envelope_integrity_only_not_zk`, integrity true, and
`accepted=false`, `proof_verified=false`, `cryptographic_proof_verified=false`.
Profile 3 is retired; it must emit `unsupported_private_membership_proof`,
`proof_written=false` and a nonzero worker exit. No private-proof success is
supported. See [the current usage guide](../docs/worker-adapter-mode.md).

Example job files:

```text
examples/jobs.merkle.jsonl
  default public diagnostics and malformed-input rejection
examples/jobs.zk_merkle.jsonl
  retired profile rejection only
examples/jobs.mixed.jsonl
  public diagnostics plus private-profile rejection; nonzero exit expected
```
