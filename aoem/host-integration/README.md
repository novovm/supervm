# Host Integration References

These files are host-side references for embedding AOEM Proof Engine directly
inside an application process.

The resident-proof references include the corrected single sources in
`../examples/`; they are not independent verifier copies. Profiles 1/2 provide
public diagnostics only (`scope=not_zk`). Profile 3 is retired and must be
rejected without successful proof output. See the
[security correction](../docs/proof-engine-v1.0-contract.md#security-correction-2026-10-04).

```text
embedded_proof_host.c
  single public diagnostic host reference

embedded_batch_proof_host.c
  batch public diagnostic and retired-profile rejection reference

embedded_asset_lifecycle_host.c
  resident public asset lifecycle host reference (use --asset-lifecycle)

embedded_confidential_transfer_host.c
  confidential_transfer_v1 host reference over existing AOEM RingCT
```

They use the existing public entry and state read path:

```text
aoem_execute_ops_wire_v1
aoem_state_read_v1
```

Public diagnostic status must say `verification_scope=envelope_integrity_only_not_zk`,
`envelope_integrity_verified=true`, `proof_verified=false`,
`cryptographic_proof_verified=false`, and `verify_status.accepted=false`.
Successful execution of an SDK reference is not a ZK or NOV business-proof
acceptance. NOVOVM continues through its original node/exec/bindings product
route, not through a new standalone proof service.

`embedded_confidential_transfer_host.c` intentionally uses the existing RingCT
and privacy-native FFI symbols:

```text
aoem_ringct_prove_v1
aoem_privacy_execute_v1
```

It is an SDK/host product profile for confidential transfer integration, not a
new Runtime Canon path and not a new proof worker default task.

No new public FFI ABI, compute op, Runtime Canon path, Graph OS path, or
dedicated LR path is introduced.
