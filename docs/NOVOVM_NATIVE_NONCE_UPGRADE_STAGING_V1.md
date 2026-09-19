# NOVOVM Native Nonce Upgrade Staging V1

Status: isolated, deterministic legacy-to-V2 state proposal and resumable
offline artifact staging. Not protocol activation, state import, AOEM
publication, chain continuation, or mainnet sign-off.

## Purpose and ownership

The [checkpoint bundle](NOVOVM_NATIVE_NONCE_CHECKPOINT_BUNDLE_V1.md) makes the
complete legacy snapshot and signed ledger history portable and checks their
commitments against explicit expected anchors. This slice consumes that
verified input and constructs a versioned proposal for the state change.
It does not open either the source database or a destination engine database.
All NOV identity, migration and journal semantics remain in SUPERVM Host
code. No AOEM repository, ABI, opcode, DLL or running service is changed.

The pure transition requires the expected source bundle digest, all five
checkpoint anchors and an explicit target protocol commitment. It repeats
bundle verification before constructing the proposal. It does not select the
target from environment variables. The CLI separately requires the supplied
target to match this binary's current environment-derived V2 protocol
commitment before staging or inspecting artifacts. A different environment
must not silently select a different upgrade target.

Only these four `module_state` fields may change:

- `native_auth_nonce_identity_scheme` becomes the V2 signer identity marker;
- `native_auth_next_nonces` becomes the completely reconstructed V2 map;
- `native_auth_nonce_reservations` becomes the completely reconstructed V2 map;
- `protocol_config_commitment` becomes the explicitly pinned target commitment.

The transformation preserves all other decoded store fields, including
business balances, account keys, historical receipts, receipt roots and state
version. It does not run another transaction or pretend a version increment
occurred. Source and proposed state roots are explicit and distinct concepts;
the proposed root is not inserted into an old block header. Original source
snapshot bytes remain bound by the source digest; the proposed store is
serialized into a separate versioned transition envelope, not emitted as a
production import file.

Transition construction additionally requires an exact complete source
snapshot schema: unknown, omitted or normalized fields reject instead of
being silently discarded or filled by a lenient Host reader. The proposed
envelope is limited to 32 MiB of encoded JSON; the checkpoint bundle retains
its own input limits. These are artifact-size limits, not hard process-memory
or execution-time bounds.

Historical canonical signer nonce reuse or incomplete/invalid history still
rejects. This is not a maximum-nonce merge, receipt deletion, transaction
renumbering, old-chain root replacement or genesis reset.

## Explicit offline commands

Use a verified bundle from a trusted coherent stopped-node copy, not live
chain directories. Establish the expected bundle digest and checkpoint
anchors independently as described in the bundle document. Paths have no
machine-specific defaults; relative paths are supported. Hash placeholders
below mean canonical lowercase 32-byte hex without a `0x` prefix.

Observe the protocol commitment calculated by this binary in the current
environment:

```text
novovmctl native-nonce-migration target-protocol
```

This is an observed configuration value, not authorization to change an
existing chain's rules. An operator must choose and approve the intended
target independently; copying observed values only establishes consistency.

Prepare in a new, dedicated directory whose parent already exists:

```text
novovmctl native-nonce-migration prepare-upgrade --bundle ./checkpoint/nonce-checkpoint-v1.bin --bundle-digest <bundle-digest> --chain-id <chain-id> --namespace-digest <namespace-digest> --legacy-protocol-commitment <legacy-protocol-commitment> --tip-block-hash <tip-block-hash> --snapshot-digest <snapshot-digest> --target-protocol-commitment <target-protocol-commitment> --workspace ./offline-upgrade/proposal-1
```

Use the same arguments with `resume-upgrade` to continue an interrupted
workspace, or with `inspect-upgrade` to revalidate it without writing
artifacts. Workspace paths must be representable as Unicode in JSON reports;
unsupported paths reject before any directory creation. `prepare-upgrade`
rejects an existing directory; it never treats a
live data directory as an empty workspace. The source bundle is verified
again on every operation, so the journal alone is not a substitute for the
pinned source input. No `--force`, import, activate or service-restart command
is provided.

## Journal and recovery contract

The dedicated workspace contains:

```text
workspace.lock
intent-<id>.json
transition.json
complete.json
```

The intent filename binds the intended inputs early, and the immutable intent
binds the source digest, checkpoint and target. A physical OS file lock on
`workspace.lock` serializes cooperating processes. Merely finding a lock file
does not mean a process still owns the OS lock. Callers must control the
workspace parent directory; this is not a defense against a hostile process
replacing paths or writing through unrelated handles.

Staging orders the intent before the transition envelope and publishes the
completion marker last, after validating the complete expected artifacts.
Recovery recomputes the exact expected bytes from the pinned input. Resume
can append missing bytes only to an exact valid prefix and only after the
existing workspace has been validated. It never truncates or overwrites
existing data. Inspect does not complete an interrupted stage.

Unknown files, symlinks, changed inputs, corrupt prefixes, invalid ordering,
or a completion marker paired with a missing/partial prerequisite reject.
If interruption occurs after directory creation but before an input-binding
intent filename exists, the unbound directory is not adopted automatically;
choose a new workspace. There is no automatic cleanup of failed workspaces.

An incomplete valid journal reports preparation, not a completed upgrade.
A valid completion marker means only that the offline transition artifact is
complete and reproducible. It does not mean an AOEM snapshot, ledger pointer,
protocol rule or chain state was activated. These facts remain false:

```text
activation_ready = false
import_performed = false
chain_canonical = false
```

The staged proposal does not establish source provenance, independently
replay historical business execution, verify AOEM execution evidence or QC,
or upgrade the local candidate ledger into a proof-sealed chain.

Recovery requirements cover process interruption at acknowledged artifact
boundaries and partial-prefix recovery. They are not a claim of tested
power-loss behavior, disk-flush atomicity or durable directory metadata on
every filesystem. Record any executed process/restart tests separately.

## Required tests and gate

The new `test_native_nonce_upgrade_staging` field starts false and becomes
true only after both commands succeed:

```sh
cargo test -p novovm-node --lib native_nonce_upgrade -- --test-threads=1
cargo test -p novovmctl native_nonce_upgrade -- --test-threads=1
```

This slice introduced the 47-field lockset. The subsequent
[upgrade authorization slice](NOVOVM_NATIVE_NONCE_UPGRADE_AUTHORIZATION_V1.md)
adds `test_native_nonce_upgrade_authorization`; the subsequent
[source prepare-QC slice](NOVOVM_NATIVE_NONCE_SOURCE_QC_V1.md) adds
`test_native_nonce_source_qc`. The later
[native seal new-view slice](NOVOVM_NATIVE_SEAL_NEW_VIEW_V1.md) adds
`test_native_seal_new_view`, extending the contract to 50 fields. The subsequent
[new-view candidate admission slice](NOVOVM_NATIVE_SEAL_NEW_VIEW_ADMISSION_V1.md)
adds `test_native_seal_new_view_admission`; producer, preflight and node-runtime
locksets now require 51 fields in the same order. Missing or false staging,
authorization, source-QC, new-view or new-view admission evidence rejects. Older reports must be
regenerated, not relabeled as current evidence.

Required cases include deterministic exact-four-field transformation,
unchanged business state/receipts/version, explicit source and target pins,
root and envelope tamper rejection, partial staging/recovery, immutable
completed replay, input/configuration drift, corrupt journals, unavailable
locks, and JSON CLI failure status. These are acceptance requirements, not
a record that the commands or real-network scenarios have passed.

## Remaining activation boundary

A production upgrade still needs an explicitly authorized protocol transition
that binds the old checkpoint and new roots to the ledger, AOEM authority and
seal/consensus domain, with recoverable publication across those owners.
The subsequent [authorization slice](NOVOVM_NATIVE_NONCE_UPGRADE_AUTHORIZATION_V1.md)
verifies a separate weighted-validator consent certificate against an
independently pinned old authority. That certificate does not establish source
canonicality or perform cross-owner publication; the staged transition and
its false activation/provenance flags are not rewritten after verification.
This slice provides neither an importer nor that cross-owner commit protocol.
Do not copy `transition.json` into a node data directory, substitute its
proposed store for authority, or change the running protocol pin to bypass
the legacy-state guard.

Local artifact and CLI passes do not establish Linux CI, physical LAN
operation, public-network readiness, long-run stability, mainnet finality or
completion of an existing chain's migration.
