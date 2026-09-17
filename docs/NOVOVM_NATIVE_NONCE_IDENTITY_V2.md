# NOVOVM Native Nonce Identity V2

Status: versioned signer nonce identity, fail-closed legacy execution boundary,
and a pure offline migration preflight. This is not a live-state migration,
protocol activation, consensus upgrade, or mainnet sign-off.

## Identity and ownership

Native ingress, authoritative execution and isolated candidate execution use
one nonce identity derived from the authenticated Ed25519 public key in the
signed transaction. Chain ID remains part of the nonce-key domain. Signature,
caller/subject authority and chain-domain verification still precede admission.
Extracting a public key is not itself signature verification.

The old derivation could use either `account:` plus the supplied account text
or `signer:` plus the decoded caller bytes. Accepted spellings such as `0x`,
`0X`, bare hex, surrounding whitespace and absent/empty account fields could
therefore select different nonce buckets for the same signer. Normalizing the
account text alone is insufficient: the same authenticated Ed25519 key can
also use its 20-byte derived address or its 32-byte public key as the caller.
V2 unifies these nonce identities by the verified public key, not by `ir.from`.

This changes only the nonce identity rule. It does not rewrite the signed wire
intent, transaction hash, caller bytes, account identifiers, balances, fee
ownership, or business-state keys. Distinct accepted intents can retain
distinct transaction hashes while competing for the same signer/chain/nonce.
An exact signed-intent replay remains distinguishable from a conflicting use
of an already consumed nonce. Maximum `u64` nonce is rejected because no next
nonce can be represented.

NOV-specific identity, nonce, account and migration rules remain in SUPERVM
Host code. This slice adds no AOEM business opcode, export, ABI, DLL change,
or sibling-repository modification.

## State and protocol compatibility

New stores carry the explicit `native_auth_nonce_identity_scheme` marker for
V2. A persisted store with a missing or empty marker is legacy, not an empty
V2 store. Unknown schemes are not accepted as V2. Existing legacy nonce maps
cannot be upgraded by attaching a marker or by relabeling their hash keys.

Execution/admission paths must fail closed on legacy or unknown schemes,
including compatibility Host paths and exact-replay shortcuts. Candidate
execution must enforce the same rule against its copied parent. Read-only
inspection is not permission to mutate old authority. Loading an old JSON or
module shard must not silently inherit a new-store V2 default.
Both nonce maps are required when deserializing a complete module snapshot;
an incomplete snapshot is rejected rather than repaired with empty maps.
A V2 native-execution shard likewise requires both maps before any fields
are applied. A nonempty RocksDB with no snapshot metadata is not new genesis.

The compiled business-protocol commitment includes the new identity rule.
Its snapshot schema and hash domain are versioned to V2.
Thus the old protocol pin and old nonce state are not silently compatible
with V2. Re-keying changes consensus state roots. Updating an environment pin
alone cannot activate a valid migration, and an existing chain must not
continue by replacing its state roots in place.

No running service, authoritative head, prepared candidate, pending pool,
ledger pointer, database or operator pin is migrated by the offline planner.
Activation requires a separately specified and verified state transition, or
an explicitly authorized new-chain/bootstrap procedure. That work remains
outside this slice.

Do not deploy this binary over a legacy running chain and merely change its
pin: rejection is intentional. Restore a valid V2 Host projection explicitly
from a verified V2 AOEM authority if necessary before resuming pending work;
an old AOEM authority is not accepted as a recovery source.

## Offline preflight API

The pure library entry point is:

```rust
tx_ingress::native_nonce_migration::plan_nonce_migration_v1(
    snapshot_json,
    raw_history,
    expected_chain_id,
    expected_namespace,
    expected_legacy_protocol,
)
```

Inputs are the complete trusted legacy Host-store JSON, complete ordered
committed signed `Execute` raw history, and independently supplied expected
chain ID, namespace digest and legacy protocol commitment. An AOEM envelope
or hashed nonce map alone is insufficient. Receipts do not reconstruct the
missing signed wire, nonce, signer key or legacy account spelling.

The planner is bounded to 16 MiB of snapshot JSON, 65,536 transactions,
64 MiB of raw history in aggregate and 1 MiB per transaction. It performs no
filesystem access, runtime calls, transaction execution or state import.

Before proposing maps it:

- checks the exact snapshot schema, chain, namespace, protocol and legacy marker;
- verifies every history transaction's signature, subject authority and chain;
- rejects duplicate intents and checks complete receipt coverage and metadata;
- reconstructs both legacy nonce maps exactly from the original signed spelling;
- rebuilds the V2 maps with continuous, checked signer nonce progression.

If aliases previously consumed the same canonical signer nonce more than
once, the planner rejects the history. It must not pick the maximum nonce,
sum alias buckets, discard one receipt or silently renumber signed history.
Incomplete history, inconsistent maps, nonce gaps, overflow and mismatched
receipt bindings likewise reject.

The report binds its exact source snapshot and ordered raw history with
domain-separated digests and includes proposed nonce maps and counts. These
are review artifacts, not a new authority snapshot, state root or protocol
pin. A successful report still carries:

```text
snapshot_provenance_verified = false
execution_results_verified = false
block_history_verified = false
qc_verified = false
activation_ready = false
import_performed = false
```

Signature and receipt-metadata checks do not verify the Host business results,
block ordering/finality, QC, or the provenance of a caller-supplied snapshot.
The operator must establish those facts independently. The pure API adds no
RPC, automatic startup migration or online fallback to legacy nonce rules.
The subsequent [checkpoint bundle slice](NOVOVM_NATIVE_NONCE_CHECKPOINT_BUNDLE_V1.md)
adds an offline CLI to export exact snapshot and ledger-history inputs and
verify their commitments against independently supplied checkpoint anchors.
That CLI does not import or activate the proposed maps and does not itself
establish execution correctness, source provenance, QC or finality.
The later [upgrade staging slice](NOVOVM_NATIVE_NONCE_UPGRADE_STAGING_V1.md)
constructs an explicit proposed state transition and resumable offline
artifacts, still without importing or activating chain state.

## Verification contract

The canonical gate requires `test_native_nonce_identity_v2`, initialized false
and set true only after this command succeeds:

```sh
cargo test -p novovm-node --lib native_nonce -- --test-threads=1
```

The subsequent checkpoint bundle slice adds
`test_native_nonce_checkpoint_bundle`, which requires the offline CLI tests.
The later upgrade staging slice adds `test_native_nonce_upgrade_staging`.
Producer, preflight and node-runtime locksets now require 47 fields in the
same order. A missing or false nonce-identity, checkpoint-bundle or
upgrade-staging field rejects. Older reports must be regenerated, not reused
as current evidence.

Required cases include same-key address/text aliases sharing one nonce,
distinct signer/chain separation, signature/subject forgery rejection,
pending/durable replay protection, legacy JSON/shard guards, nonce overflow,
and migration acceptance/rejection against exact complete history. Existing
candidate execution parity/recovery tests remain separately required.
Pending admission retains the existing exact durable nonce floor: it does not
add a future-nonce queue. A next nonce cannot enter pending before its preceding
nonce has been durably consumed; ordered authenticated batch execution remains
able to evaluate consecutive nonces against a candidate-local map.

These are verification requirements, not a record of executed tests. Record
actual output separately. Local passes do not establish Linux CI, physical
LAN devices, public-network operation, long-run stability or chain activation.
