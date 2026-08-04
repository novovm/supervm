# NOVOVM Verifiable Block Candidate Durable Ledger V1

Status: active implementation contract, not a mainnet-finality signoff

Scope owner and NOV business-transition computation: NOVOVM Host

Domain-neutral graph execution and authoritative state persistence: AOEM Engine
through its generic, domain-agnostic capability surface

## 1. Purpose and exact boundary

This contract defines the first durable NOVOVM block-shaped artifact produced
from an AOEM-owned native execution batch. The artifact is a **local canonical
execution candidate**: its inputs have one canonical order, its encoding is
deterministic, and its execution roots have been checked against AOEM durable
readback.

The word `canonical` in that phrase describes the local execution form only.
It does not mean that the network selected the candidate as the canonical
chain block. Every V1 candidate created by this milestone MUST be reported as:

```text
candidate_kind = local_unsealed_execution_candidate
local_execution_canonical = true
chain_canonical = false
proof_sealed = false
safe = false
finalized = false
```

AOEM execution success, AOEM persistence success, or a locally reproducible
candidate hash MUST NOT change any of the last four values to `true`.

This milestone deliberately does **not** implement:

- a validity proof or proof-seal verifier;
- a quorum certificate (QC), voting, fork choice, or consensus finality;
- promotion from candidate state to a network canonical/safe/finalized head;
- a production block interval, gas/resource market, retention time, pruning
  policy, or hardware-capacity target. V1 does enforce a defensive maximum of
  1,024 transactions and 2 MiB of raw transaction body per candidate; those
  are implementation safety ceilings, not signed throughput targets.

It creates the durable input that those later stages can seal. A future proof
seal must bind this candidate's hash; proof validity and consensus finality
remain separate decisions.

## 2. Ownership discipline

The ownership split is normative:

```text
NOVOVM Host
  - authenticates ingress and fixes transaction order
  - fixes and validates the block execution context
  - computes every NOV business rule and resulting business-state transition
  - lowers the deterministic result to opaque, domain-neutral graph writes
  - builds the candidate header/body and block indexes
  - owns block scheduling, proof policy, consensus, query semantics

AOEM Engine
  - admits and executes the domain-neutral atomic graph writes
  - owns authoritative state/receipt persistence and durable readback
  - returns or preserves the state/receipt commitments already used by the
    AOEM-owned native state envelope
```

No NOV transaction, block, token, governance, receipt, QC, or fork-choice
business rule may be added to AOEM. This contract requires no NOV-specific
AOEM opcode, key namespace, schema, DLL export, or AOEM repository change.
The Host may pass opaque deterministic inputs and writes through AOEM's
domain-agnostic surface, but it remains the only owner and interpreter of NOV
transaction semantics and the only component that computes NOV business-state
transitions. AOEM does not interpret or compute NOV balances, fees, treasury,
governance, receipt, or block rules.

## 3. Fixed Host execution context

One batch is executed under exactly one `NovBlockExecutionContextV1`:

```text
chain_id: u64
block_height: u64
parent_block_hash: [u8; 32]
slot: u64
timestamp_unix_ms: u64
```

The V1 canonical wire is fixed-width:

```text
NBX1 || version:u8 || chain_id:u64le || block_height:u64le ||
parent_block_hash:[u8;32] || slot:u64le || timestamp_unix_ms:u64le
```

Its commitment is a domain-separated SHA-256 digest of that wire. The context
is constructed once before execution and passed unchanged to every transaction
in the candidate.

The Host MUST enforce these rules:

1. `chain_id` and `block_height` are non-zero.
2. Height one must use the zero block-parent hash. Later heights require a
   non-zero parent hash and must extend the selected durable local execution
   parent. Bootstrapping from an existing AOEM authority binds that authority
   through the separate `aoem_parent` record and exact migration anchor; it
   never repurposes the block-parent field.
3. Transaction order comes from the candidate body. Arrival time, local queue
   update time, thread scheduling, map iteration order, and machine identity
   may not reorder it.
4. Execution-visible time comes only from `timestamp_unix_ms`. Once the
   context exists, `SystemTime::now()` or another local clock may not influence
   state, receipts, fees, policy windows, trace commitments, or candidate
   identity.
5. Wall-clock values may still be recorded as explicitly non-consensus
   diagnostics, but they must not enter any root or immutable candidate field.

The upstream block producer will eventually validate slot and timestamp rules.
This V1 contract does not define the duration of a slot or the interval between
blocks. The current local candidate scheduler derives a fresh context from its
local clock when no prepared intent exists. That is sufficient only for a
single-node unsealed candidate; it is not a four-validator agreement rule. A
future proposer/proof-seal stage must distribute one shared, verifiable context
before multiple validators can be expected to derive the same candidate.

`block_execution_context` is producer-internal protocol input. Public RPC must
not accept a caller-supplied context or turn an internal-call marker into
authorization. A production node obtains the context only from its trusted
block-production lifecycle and rejects public attempts to override it.

## 4. Execution roots and their meanings

The candidate MUST reuse the real commitments from the AOEM-owned execution
path. It MUST NOT derive a synthetic state root from the candidate hash.

### 4.1 `pre_state_root`

`pre_state_root` is the native semantic state commitment at the AOEM-owned
parent state from which the ordered batch executes. It is obtained from the
verified prior AOEM-owned head/readback and recomputed from the loaded state.
Both values must match before execution begins.

For the current native state format this is the domain-separated digest
produced by `native_semantic_ledger_state_digest_v1` under
`novovm-consensus-native-state-wire/v1`. The V3 projection is encoded with a
type-tagged, length-framed canonical value codec whose object keys are sorted.
It binds the AOEM semantic-ledger sequence/head and the native protocol-config
commitment, while excluding full local mirror records and machine-local AOEM
scheduling/fallback diagnostics. It is a commitment to the Host-defined native
state projection preserved by AOEM; it must not be misrepresented as an
AOEM-native Merkle membership proof.

The prior `novovm-native-state-json-legacy/v2` root remains readable only for
state migration and recovery compatibility. A newly committed durable block
must use the V3 state codec and the V2 receipt codec; legacy root semantics
cannot enter a new candidate.

### 4.2 `post_state_root`

`post_state_root` is the corresponding state commitment after the entire
ordered batch succeeds. The value currently built for the AOEM native batch
result and the value persisted in the AOEM-owned envelope are the same value.
The candidate may be constructed only after AOEM readback recomputes and
confirms it.

### 4.3 `cumulative_receipt_root`

`cumulative_receipt_root` is the existing AOEM-owned receipt-ledger commitment
after the batch. In the current format it commits to the complete cumulative
`store.receipts` map, including receipts from earlier execution batches. It is
persisted in the AOEM-owned envelope and must pass AOEM readback verification.
The map is traversed in deterministic transaction-hash order and each value is
represented by the same dedicated consensus receipt commitment described
below; the root does not commit the complete runtime receipt serialization.

This root proves continuity of the local cumulative receipt ledger. It is not
the receipt root of the current block and MUST NOT be exposed as one.

### 4.4 `block_receipt_root`

Every candidate also has an independent `block_receipt_root`. It commits only
to the current candidate's ordered, full native receipts. The committed
receipt representation includes all consensus-relevant receipt fields, logs,
fee and policy metadata, failure information, and AOEM semantic commit
metadata; a tuple containing only transaction hash and success status is not
sufficient.

The receipt consensus codec is the dedicated
`novovm-consensus-receipt-wire/v1` postcard wire. It serializes an explicitly
selected, ordered receipt projection. JSON-valued log data is first encoded by
a type-tagged, length-framed canonical value codec whose object keys are sorted.
The codec does not serialize the complete runtime receipt structure.

Host hardware and operational fields are not consensus receipt fields. In
particular, hardware/thread recommendations, ingress worker counts,
`required`/fallback policy, chunking choices, persistence backend, filesystem
paths, and other runtime diagnostics MUST NOT enter a receipt commitment,
receipt root, execution-evidence commitment, or candidate hash. If a generic
AOEM plan identifier or metric is retained as consensus evidence, its derivation
must be fixed by the protocol and independent of environment configuration.

For each full receipt, V1 computes a domain-separated commitment to the
dedicated postcard wire. The current-block receipt root then binds the ordered
transaction identity and receipt commitment as follows:

```text
receipt_commitment[i] =
  H("novovm-full-native-receipt-commitment-v1\0" ||
    consensus_receipt_postcard_wire[i])

block_receipt_root =
  H("novovm-native-block-receipt-root-v1\0" ||
    receipt_count:u64be ||
    repeated(tx_hash:[u8;32] || receipt_commitment:[u8;32]))
```

Adding an unrelated historical receipt may change
`cumulative_receipt_root`, but it may not change `block_receipt_root` for an
otherwise identical candidate. Reordering, adding, deleting, or changing any
current-block receipt must change `block_receipt_root`.

## 5. Candidate artifacts

### 5.1 Immutable header

The durable header contains at least:

```text
schema/version
candidate_kind
execution_context
execution_context_commitment
parent_block_hash
transaction_count
tx_root
body_root
receipt_count
pre_state_root
aoem_parent batch/result identity, roots, root codecs, and state version
post_state_root
post_state_root_codec
block_receipt_root
cumulative_receipt_root
cumulative_receipt_root_codec
aoem_batch_id
aoem_batch_result_id
aoem_expected_output_commitment
aoem_readback_verified
```

`candidate_hash` is the domain-separated hash of a versioned canonical binary
encoding of the immutable header payload. It must bind every field above. It
must not rely on JSON object ordering or unframed byte concatenation.

Lifecycle flags are stored alongside the header but are not silently mutated
into proof or finality claims. This milestone writes the fixed values from
Section 1.

### 5.2 Body and receipt commitments

The durable body preserves the exact ordered raw transactions and their hashes.
Its `tx_root` and `body_root` are recomputed during reads and recovery. The
ledger stores an ordered commitment for every full current-block native
receipt and recomputes `block_receipt_root`; the full receipts remain in the
AOEM-owned cumulative state and its verified Host query projection. Receipt
queries must load that full receipt and verify it against the block-ledger
commitment before returning it.

The body order is the execution order. A validator or later proof worker must
be able to replay the same parent state, body, and execution context without
consulting local pending-queue timestamps.

The AOEM-owned envelope may retain non-consensus runtime diagnostics. Its raw
serialized bytes, envelope digest, or domain-neutral graph identifier may
therefore differ across machines. Such values are local durability evidence,
not block identity. Given the same parent state, ordered body, protocol
configuration, and execution context, every machine MUST still derive the same
pre/post state roots, both consensus receipt roots, execution-evidence
commitment, and candidate hash.

The AOEM storage namespace is local isolation configuration. It is checked when
opening the authoritative store, but it is deliberately excluded from the
consensus AOEM batch identity. Conversely, every environment value that can
select native business policy is captured with the compiled protocol defaults
in `novovm-native-business-protocol-config/v1`; its canonical commitment is
persisted in state and therefore enters the V3 state root. Once bound, local
configuration drift fails closed instead of silently producing a different
block. Whenever the AOEM production ownership gate is enabled, every machine
MUST set the same expected 32-byte commitment with
`NOVOVM_NATIVE_PROTOCOL_CONFIG_EXPECTED_COMMITMENT`. Startup and the production
batch/pending paths reject a missing, malformed, or mismatched pin before
opening production authority/ledger state. Leaving it unset is accepted only
while the production gate is disabled for local development and does not
qualify as four-machine deployment evidence.

Operators derive the commitment from the exact binary and environment with:

```text
NOVOVM_NODE_MODE=native_protocol_config_commitment novovm-node
```

The command does not open AOEM state. Its reported pin must replace the package
placeholder and be identical on all four machines before production ownership
is enabled. The value is protocol/environment specific, not a filesystem-path
commitment.

AOEM execution evidence uses the fixed
`novovm-aoem-native-batch-consensus-evidence/v1` codec. That evidence binds the
V3 state-root codec and V2 receipt-root codec as well as batch/result identity,
ordered per-transaction receipt evidence, roots, close evidence, and state
version. Persistence backend labels and filesystem paths remain diagnostic and
do not enter it.

The prepared record also durably binds the AOEM batch id and its
`expected_output_commitment`. Readback recomputes the canonical inclusion
commitment, durable close commitment, and batch-result id. A syntactically
valid 32-byte close value is not sufficient. The same expected-output binding
is retained in the immutable header/evidence and candidate hash.

### 5.3 Indexes and local candidate head

The Host durable ledger maintains, at minimum:

- `candidate_hash -> header`;
- `candidate_hash -> ordered body and receipt bundle`;
- `(chain_id, block_height) -> candidate_hash list or selected local candidate`;
- `tx_hash -> candidate_hash and transaction index`;
- `(candidate_hash, tx_hash) -> receipt index`;
- `chain_id -> local_execution_candidate_head`.

The local candidate head is not the chain canonical, safe, or finalized head.
Existing network/RPC heads with those meanings must not be advanced by this
write.

## 6. Durable write protocol

Candidate creation follows this order:

1. Validate the fixed execution context and the exact ordered raw body.
2. Durably retain the prepared body/execution intent so a crash cannot force a
   new transaction order or timestamp. The prepared identity also binds the
   exact prior AOEM batch/result identity, state/receipt roots and codecs, and
   state version.
3. Load and verify the AOEM-owned parent; require the V3 state/V2 receipt
   codecs and exact Host projection parity, then capture `pre_state_root`.
4. The Host computes the full NOV business-state transition under the fixed
   context and lowers its result to domain-neutral atomic graph writes.
5. AOEM executes and persists those writes as the AOEM-owned envelope, then the
   Host verifies durable readback of
   `post_state_root` and `cumulative_receipt_root`.
   Before publication, after AOEM readback, and again during crash recovery,
   the Host independently recomputes the current production-acceptance gate;
   the envelope's `production_accepted` marker is not accepted as proof by
   itself.
6. Build the full ordered receipt bundle and `block_receipt_root`.
7. Build and hash the immutable candidate header.
8. In one atomic Host ledger batch, publish the completed header, body/receipt
   reference, height/transaction/receipt indexes, and local candidate head.

The AOEM publish and Host block-ledger publish execute while the same
per-store Host execution lock remains held. A competing worker cannot close a
pending transaction merely because the AOEM receipt became visible first. If
a task or process stops after AOEM persistence but before Host publication, a
later pending tick detects the retained prepared candidate, verifies the AOEM
completion, publishes it idempotently, and only then closes the pending item.
If AOEM has not completed, the next tick executes the exact raw body and order
stored in the prepared record. It neither depends on the volatile pending
payload cache nor appends transactions that arrived after preparation.

Readers must see either the previous complete head or the new complete head.
They must never see a head whose header, body, receipts, or indexes are
missing. An unreferenced prepared body is recoverable garbage; it is not a
candidate and is not query-visible as a block.

AOEM persistence establishes execution-state durability. The Host ledger
establishes block-artifact durability. Neither operation establishes network
finality.

## 7. Restart recovery

On restart the Host must load the durable local candidate head instead of
reconstructing height one, a zero parent, transaction order, or time from
process memory. Startup may restore the pending lifecycle of already-known
transactions as `IncludedNonCanonical`, but it does not inject an unsealed
candidate into the shared network head/header/body or canonical-chain views.
Historical blocks, bodies, transaction locations, and receipt locations remain
in RocksDB and are loaded through bounded block-ledger query methods; startup
must not copy the complete history into memory.

Recovery validates, in fail-closed order:

1. the head points to an existing header;
2. canonical header encoding recomputes `candidate_hash`;
3. the body exists and recomputes `body_root`, `tx_root`, and transaction
   count;
4. the receipt bundle exists and recomputes `block_receipt_root` and receipt
   count;
5. height and parent linkage are continuous with the durable parent record;
6. the referenced AOEM batch result exists and its readback-verified
   `post_state_root`, root codecs, `cumulative_receipt_root`, state version,
   expected-output/close-proof binding, current production gate, and canonical
   execution-evidence commitment match the header;
7. all height, transaction, and receipt indexes resolve back to this candidate.

An unfinished execution intent must be resumed with its original body and
execution context, completed idempotently from the matching AOEM commit, or
quarantined with an explicit recovery error. Recovery must not substitute the
current clock, select a new queue order, fabricate roots, or mark the artifact
safe/finalized.

Corrupt or mismatched records fail closed. Automatic deletion or best-effort
promotion is not permitted.

An existing AOEM authority head together with a missing/empty block ledger is
not treated as a fresh genesis automatically. The operator must run an
explicit migration with
`NOVOVM_ALLOW_NATIVE_BLOCK_LEDGER_BOOTSTRAP_FROM_AOEM=true` long enough to
prepare and commit the anchored first local candidate. The Boolean alone is
not authorization: `NOVOVM_NATIVE_BLOCK_LEDGER_BOOTSTRAP_AOEM_ANCHOR_COMMITMENT`
must equal the domain-separated commitment of the exact AOEM chain/namespace,
batch/result ids, expected output, state/receipt roots and codecs, state
version, and protocol-config commitment. Startup rejects an empty ledger plus
an AOEM head immediately if either value is absent or mismatched. These values
are offline migration configuration, not RPC fields, and should be removed
after the anchored migration. A changed filesystem path, advanced AOEM parent,
or deleted ledger is otherwise a fail-closed recovery event, not permission to
restart at height one.

The migration also writes a consumed marker through AOEM's domain-neutral
atomic graph surface after the exact prepared intent is durable. A retained
prepared intent may finish an interrupted migration, but deleting or replacing
the Host ledger cannot reuse the consumed anchor as a new height-one
authorization. The ledger itself durably binds the AOEM chain, local namespace,
and protocol-config commitment; once that binding, a prepared candidate, a
ledger head, a Host authority binding, or an AOEM authority head exists,
disabling the production gate is rejected as an ownership downgrade.

## 8. Query contract

Candidate-aware queries must support:

- latest local execution candidate for a chain;
- candidate header/body by `candidate_hash`;
- candidates by height;
- transaction and full receipt lookup by `tx_hash`;
- the four roots and AOEM batch references;
- recovery/readback status and the explicit lifecycle flags.

Every response must make the trust class visible. A V1 response cannot omit or
reinterpret:

```text
proof_sealed = false
safe = false
finalized = false
chain_canonical = false
```

Generic `latest candidate` queries may return this artifact. Queries whose
contract means `safe`, `finalized`, or network-canonical must not return it as
if it had crossed those boundaries. A later sealed-block record should refer
to the immutable `candidate_hash` rather than rewriting candidate history.

Persistence paths are node configuration. Public query parameters cannot
select or create another ledger database. Tests and trusted startup code use
an explicit internal path channel; production RPC always resolves the
node-configured/default path. AOEM-owned authority, AOEM core
`AOEM_PERSISTENCE_PATH`, Host projection/RocksDB, block ledger, semantic
mirror, and unified-account stores must resolve to disjoint paths. Equality
and ancestor/child nesting fail closed after existing symlink/junction
ancestors are resolved. Persistence configuration containing a `..` component
is rejected outright, avoiding symlink-before-parent traversal ambiguity.

## 9. Execution order, concurrency, block period, and capacity

Determinism requires one agreed transaction order and one atomic publication
order; it does not require every stage of the node to run on one CPU thread.
Network receive, authentication, decoding, AOEM pre-processing, proof work, and
transactions with proven-disjoint state access may run concurrently. Their
results must still be committed according to the candidate body order and must
produce the same roots as deterministic replay.

The current generic native business-state mutation loop uses one ordered Host
store and therefore commits those mutations serially. This contract does not
claim Solana-style account-lock parallel execution. A later parallel milestone
must add deterministic read/write access declarations, conflict partitioning,
and ordered merge/replay checks before it can safely move non-conflicting
business execution into parallel AOEM partitions.

This V1 ledger does not hardcode a block period. The native execution tick is
a queue/lifecycle scheduling interval and is not a consensus block clock.
Candidate production starts only when an upstream Host rule supplies a valid
execution context and ordered body. Slot duration, empty-block behavior,
production transaction/byte limits, gas/resource limits, and adaptive
production rules require a later protocol/governance decision.

V1 rejects a candidate above 1,024 transactions or 2 MiB of raw transaction
body. This is a corruption/resource-exhaustion guard, not the final gas model,
TPS promise, proof-capacity target, or retention policy. Operators must size
storage from measured workload and the later retention policy. A planning
estimate is:

```text
candidate_bytes_per_second ~=
  tx_per_second * (average_raw_tx_bytes + average_full_receipt_bytes +
                   average_tx_and_receipt_index_bytes)
  + candidates_per_second * average_header_and_height_index_bytes

required_local_bytes ~=
  retention_seconds * candidate_bytes_per_second
  + AOEM_authoritative_state_and_history_bytes
  + snapshots_and_future_proof_bytes
```

Replication, compaction headroom, database write amplification, backups, and
future proof artifacts must be added to that estimate. Capacity is therefore
an observed deployment parameter, not a protocol constant in this milestone.

The production pending selector clamps any machine-local AOEM batch setting to
1,024 transactions and fills candidates by a deterministic raw-byte prefix no
larger than 2 MiB. A suffix that would cross the byte ceiling is deferred to
the next candidate. A single transaction larger than the entire ceiling is
rejected and its nonce reservation is released; it cannot permanently block
the queue head. These rules are production-liveness guards, not throughput
claims.

## 10. Acceptance gates for this milestone

The implementation is complete only when tests demonstrate:

1. identical parent AOEM state, ordered body, and execution context produce
   identical pre/post roots, both receipt roots, and candidate hash on isolated
   runs despite different real wall clocks;
2. changing any context field, transaction order/body, receipt, or root changes
   the candidate hash or fails validation;
3. `post_state_root` and `cumulative_receipt_root` exactly match AOEM durable
   readback, while `block_receipt_root` is independent of historical receipts;
4. no QC/proof seal leaves `chain_canonical`, `proof_sealed`, `safe`, and
   `finalized` false and does not advance the network finalized head;
5. restart recovers the same height, parent, body, indexes, roots, and local
   candidate head;
6. crash injection before AOEM commit, after AOEM commit, and before/after the
   Host atomic publish either completes idempotently or leaves the prior
   complete head visible;
7. query results distinguish local candidates from safe/finalized blocks;
8. AOEM capability inspection confirms the generic domain-agnostic boundary
   and no NOV-specific AOEM business surface was introduced.

Until all gates pass, the durable ledger remains an implementation target, not
evidence of proof sealing or mainnet finality.

## 11. Candidate graph extension

Same-height competing candidates, observed-vs-local trust classes,
parent/children indexes, durable abort tombstones, and the explicit boundary
before QC/fork choice are specified separately in
[`NOVOVM_UNSEALED_BLOCK_CANDIDATE_GRAPH_V1.md`](NOVOVM_UNSEALED_BLOCK_CANDIDATE_GRAPH_V1.md).
That sidecar extension does not promote any candidate to chain canonical,
proof-sealed, safe, or finalized state.
