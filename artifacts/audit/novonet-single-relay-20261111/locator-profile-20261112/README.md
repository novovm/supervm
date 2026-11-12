# Preserve the approved relay allowance through native discovery

Base rejected candidate: `a2d7af7fb867936ca7e062fa32e91ea43e871345`.

Both package builds, copied runtime verification and seven packaged identity
tests passed. The expanded original owner gate then failed identically on both
platforms:19 passed,3 failed. The failures were:

- `all_native_owners_cold_reopen_from_frozen_bootstraps_and_changed_seed_coordinates`
- `cold_private_only_checkpoint_keeps_authenticated_seed_as_dial_hint`
- `native_owner_discovers_peers_through_shared_rpc_then_delivers_original_objects`

Each reported `native locator projection would change signed fields`. The
constrained single/double relay tests passed in that same run; they did not
exercise this signed-discovery projection. No candidate Full was dispatched,
no archive was qualified and no live node changed.

## Cause and minimal correction

The128KiB candidate correctly signed131072 into its original endpoint declaration,
but the compact discovery projection rebuilt the generic endpoint with65536.
Its original exact-field check rejected the change. Simply disabling that check
would still reconstruct the wrong signed bytes at the receiver, and would weaken
the contract. This is a production integration omission, not a test-fixture bug.

The original NODE_PUT/NODE_GET RPC and native locator owner now recognize two
explicit compact profiles:

| Marker | Reconstructed signed allowance | Other fields |
| --- | ---: | --- |
| `NVLOCR01` |65536 bytes/min|original bytes/semantics preserved|
| `NVLOCR02` |131072 bytes/min|same four-session, priority-zero endpoint profile|

No additional bytes or discovery-body allowance are needed. The original record
signature is retained and verified over all reconstructed fields. Switching the
marker cannot promote/downgrade a signed allowance; unsupported profiles, other
allowances, changed session count and priority are rejected. The node's actual
shared budget and signed declaration reference the same exported V2 allowance.
The generic Iroh endpoint default stays64KiB; no other consumer is increased.

V1 remains necessary for currently valid signed seeds/caches, not as a second
network or silent lower-security fallback. Old programs reject V2; the new
candidate must be deployed coherently before accepting four-host double-relay
results. No NOVOCHAT code, NovoRUDP frame, identity/signature suite, chain pins,
replay rule, expiry, lease, connection count, ledger or AOEM implementation changes.

## Verification status

Windows/Linux compact-profile tests each pass2/2, including unchanged original
signature/domain/key/expiry rejection and new profile-substitution counterexamples.
The original owner group passes22/22 on each platform (Windows277.83s,
Linux264.42s), including all three previously failing cases and actual local
single/double-hop owner delivery. Network and node strict Clippy with
`-D warnings` pass on both platforms. CI scope tests pass27/27.
Exact commands, source hashes and retained old/new logs are in
[results.json](results.json) and [local-evidence.zip](local-evidence.zip).
The initial local runner argument-shape failure is also retained; it executed
no tests and is not counted as a product result.

The three original failure cases and both locator tests are now explicitly
enumerated and run in the existing Full steps; ordinary push scheduling is
unchanged. No assertion, failure evidence or production limit is removed.

This is local correction evidence, not a new fixed package, Full success,
deployment, C heap-crash root cause or real double-relay DONE claim.
