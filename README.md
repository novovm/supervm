# NOVOVM v2026

External brand: `NOVOVM`  
Technical short name: `NVM`  
Execution engine: `AOEM Engine` (Powered by AOEM Engine)

Note: `SuperVM` is retained as an internal historical codename only.

NOVOVM is a **decentralized infrastructure operator** for the Web3 era. It provides composable, metered, and verifiable execution and settlement capabilities. It is not “another public blockchain,” but a general-purpose execution infrastructure for a multi-chain, heterogeneous ecosystem.

## What it is / What it isn’t

**NOVOVM is:**
- A Web3 infrastructure layer that offers execution, verification, settlement, and resource pricing
- Built around `AOEM`, a high-concurrency execution kernel with stable P99 latency
- Trust-minimized through `zkVM`-based verifiable execution and proof aggregation

**NOVOVM is not:**
- A monolithic high‑TPS public blockchain
- A single‑purpose cross‑chain bridge
- A network sustained primarily by inflation

## Architecture overview

- **Unified execution kernel (`AOEM`)**
	- Semantic concurrency with `OCCC` as the primary execution path
	- `OCC` as a validation baseline and `MVCC + 2PC` as a strict safety fallback
- **Three‑channel routing layer**
	- Standard Path / Consensus Path / Privacy Path
	- Transparent to developers; acts as a QoS routing mechanism
- **Four‑layer network (L1–L4)**
	- L1: Finality & arbitration
	- L2: Execution & proof workers
	- L3: Edge & routing nodes
	- L4: Clients & devices
- **L0 security kernel**
	- Zero‑knowledge verification (Groth16, Bulletproofs, RingCT / MLSAG)
	- Post‑quantum readiness (multi‑level ML‑DSA signatures)

## Core design goals

1. Execution‑first: execution is a first‑class capability, not a byproduct of consensus
2. Verification over trust: correctness is proven, not assumed
3. Meterable & settleable resources: compute, storage, and bandwidth are economic resources
4. Stable P99 latency under high throughput

## Verifiable execution path

NOVOVM separates execution, proof, and consensus:
- **Execution** is handled by AOEM
- **Correctness** is proven via `zkVM`
- **Consensus** is limited to finality and arbitration

Proof generation is decoupled from execution:
- Proofs can be **lazy**, **batched**, and **recursively aggregated**
- `RISC0 zkVM` proves correctness, `Halo2` aggregates proofs

Verification is **value‑aware**:
- Standard execution (no immediate proof)
- Auditable execution (on‑demand proofs)
- High‑value execution (mandatory zk proofs)

## Governance & evolution

NOVOVM is built for long‑term infrastructure evolution:
- Upgradable protocols
- Post‑quantum readiness
- Layered governance

## Economics: execution‑driven, not inflation‑driven

NOVOVM’s economic model is anchored in **real, verifiable execution demand**:
- Execution is economic activity
- Compute, storage, and bandwidth are **settleable labor**
- Value capture is service‑driven, not speculative

**Native token boundaries (explicitly limited):**
- Unit of account for execution and service settlement
- Governance participation and risk‑bearing
- **Not** equity, **not** income‑sharing, **not** a stablecoin

**External value is required:**
- External assets (stablecoins, fiat‑pegged assets, other chains) provide pricing references
- Token circulation requires verifiable external value inflows
- No issuance driven purely by time or internal loops

**Dual‑track pricing:**
- Rigid redemption / clearing track
- Market trading / liquidity track

## Performance (whitepaper‑reported)

AOEM’s triple breakthrough toward a distributed execution plane:
- Compute Plane (L0): **8M+ TPS**
- Coordination Plane (L1): **4M+ TPS**
- Network Plane (L4): **1M+ msgs/s**

## Developer interface & SDK

NOVOVM exposes a **unified Execution API** rather than exposing concrete execution engines.
Developers declare:
- Execution target (function, transaction, or task)
- Required consistency and security guarantees
- Required privacy / verification properties

The system automatically handles:
- Three‑channel routing selection
- Execution and proof generation
- Settlement and verifiable commitment of results

NOVOVM is **WASM‑first** and multi‑language:
- Rust, C/C++, Zig, AssemblyScript, and more
- Portable, verifiable execution
- Reuse of existing high‑performance system code

## Privacy & security

Privacy and security are treated as **infrastructure primitives**, not optional features:
- Verifiable execution via `zkVM`
- Privacy proofs (e.g., Bulletproofs, Groth16, RingCT / MLSAG)
- Execution‑proof decoupling with on‑demand / batched / aggregated proofs
- Security boundaries backed by post‑quantum readiness

## AOEM FFI transparency

For capability evaluation (to avoid “black box” misreads), use:

- `docs_CN/AOEM-FFI/README.md`
- `docs_CN/AOEM-FFI/SUPERVM-AOEM-CAPABILITY-AUDIT-V1-2026-05-23.md`
- `docs_CN/AOEM-FFI/SUPERVM-AOEM-PROOF-ENGINE-HOST-INTEGRATION-V1-2026-05-23.md`

Current AOEM integration is a single-layer FULLMAX host package, not a Proof-only bundle and not a standalone AOEM platform service. The Proof Engine path is one capability inside that package:

```text
SUPERVM host -> aoem_execute_ops_wire_v1 -> compute.zk.resident_proof_v1 -> aoem_state_read_v1
```

The same AOEM package also carries the FULLMAX capability surface used by SUPERVM, including primitive operators, GPU-adaptive paths, RocksDB persistence sidecar, WASM/Wasmtime sidecar, zkVM executor sidecar, ML-DSA, KMS/HSM, RingCT, Bulletproof, Groth16, classic hashes, and classic signature verification. See the capability audit before making product or security claims.

Current runtime baseline: `AOEM FULLMAX Runtime Baseline 2026-05-23`. Windows and Linux have been refreshed from newly generated AOEM `SUPERVM v1.2 FULLMAX` bundles and are the included/verified runtimes. macOS runtime artifacts are currently not bundled; old macOS platform binaries were removed to avoid stale FULLMAX claims. See the AOEM capability audit for the exact status.

## Ecosystem positioning

NOVOVM is a **collaborative infrastructure layer**:
- It does not replace existing chains
- It augments them with execution, verification, and settlement services
- Value capture is service‑driven, not sovereignty‑driven

## Compatibility & chain relationships

NOVOVM complements heterogeneous systems rather than competing for their state:
- **L1 public chains**: execution outsourcing / clearing collaboration
- **L2 / rollups**: shared execution and proof infrastructure
- **Specialized chains**: plugin chains / protocol recomposition
- **Private chains**: verifiable execution with privacy‑preserving settlement

## One‑sentence summary

NOVOVM is not about building a faster blockchain—it is about building sustainable, verifiable infrastructure for Web3.

## Mainline nightly soak gate

EVM plugin maintenance mode (running on NOVOVM host) includes a dedicated nightly soak gate (separate from the main CI gate):

- Workflow: `.github/workflows/mainline-nightly-soak.yml`
- Runner target: `self-hosted`
- Default soak profiles: `6h,24h`
- Gate binary: `cargo run -p novovm-node --bin supervm-mainline-nightly-gate`

Key artifacts:

- `artifacts/mainline/mainline-nightly-soak-gate-report.json`
- `artifacts/mainline/mainline-soak-6h.json`
- `artifacts/mainline/mainline-soak-24h.json`
- `artifacts/mainline/mainline-duty-report-nightly.md` (generated in nightly workflow)

Operations SOP (CN):

- `docs_CN/NOVOVM-NETWORK/NOVOVM-EVM-NIGHTLY-SOAK-SOP-2026-04-17.md`
- `docs_CN/CURRENT-AUTHORITATIVE-ENTRYPOINT-2026-04-17.md`

EVM protocol-observable equivalence scope (CN):

- `docs_CN/Adapters/EVM/NOVOVM-EVM-PROTOCOL-OBSERVABLE-EQUIVALENCE-V1-2026-06-07.md`
- v2 RPC projection gate: `cargo test -p novovm-node evm_protocol_observable_equivalence_geth_rpc_blackbox_projection_gate_v2 -- --nocapture`
- v2b real geth block diff gate: `cargo test -p novovm-node evm_protocol_observable_equivalence_geth_real_block_diff_gate_v2b -- --nocapture` (raw tx RLP present => `transactionsRoot` matches geth fixture)
- v3 RLPx tx ingress gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_tx_ingress_gate_v3 -- --nocapture` (real RLPx `Transactions` frame => remote pending tx with raw RLP broadcast candidate)
- v3 RLPx tx outbound gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_tx_outbound_broadcast_gate_v3 -- --nocapture` (local pending raw tx => real RLPx `Transactions` broadcast)
- v3 RLPx pooled tx gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_pooled_tx_gate_v3 -- --nocapture` (real `NewPooledTransactionHashes` => `GetPooledTransactions` => raw `PooledTransactions` materialized)
- v3 RLPx pooled tx response gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_pooled_tx_response_gate_v3 -- --nocapture` (real `GetPooledTransactions` => local raw tx `PooledTransactions` response)
- v3 RLPx block body import gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_block_body_import_gate_v3 -- --nocapture` (real `BlockHeaders`/`BlockBodies` => body transaction trie root validated + native body snapshot)
- v3 RLPx receipts gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_receipts_gate_v3 -- --nocapture` (real `BlockHeaders`/`BlockBodies` => follow-up eth/70 `GetReceipts` => complete `Receipts` parsed + receipt count/root validated + native receipt snapshot + local `GetReceipts` replay + empty no-withdrawal stateRoot continuity validation when parent is retained)
- v3 RLPx empty-body receipt gate: `cargo test -p novovm-network rlpx_empty_body_materializes_empty_receipts_without_remote_receipts -- --nocapture` (materialized empty body with empty `receiptsRoot` creates an empty native receipt snapshot locally, avoiding a long sync stall while waiting for a remote `Receipts` response)
- v3 RLPx missing-receipts recovery gate: `cargo test -p novovm-network real_rlpx_worker_recovers_missing_receipts_before_new_header_pull -- --nocapture` (after a peer disconnect leaves latest header/body available but receipt missing, the next ready RLPx worker rebuilds pending receipt state and sends `GetReceipts` before a new header pull)
- v3 RLPx same-tick sync dispatch gate: `cargo test -p novovm-network real_rlpx_peer_worker_ingests_runtime_native_snapshots -- --nocapture` (after Status succeeds, the real RLPx worker dispatches the first `GetBlockHeaders`/sync request in the same tick instead of waiting for the next scheduler tick)
- v3 RLPx reorg gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_reorg_gate_v3 -- --nocapture` (real RLPx branch switch => canonical reorg + pending tx back to broadcast candidate)
- v3 RLPx BAL plugin response gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_response_gate_v3 -- --nocapture` (no-snap plugin path `GetBlockAccessLists` => protocol-valid `BlockAccessLists` response; canonical/materialized local BAL RLP returned, unavailable hashes use missing sentinel)
- gateway RLPx eth/71 capability gate: `cargo test -p novovm-evm-gateway rlpx_gateway_capability_guard_advertises_eth71_and_prefers_latest -- --nocapture` (gateway hello profiles advertise/select `eth/71`, fall back to `eth/70`)
- gateway RLPx eth/71 BAL code gate: `cargo test -p novovm-evm-gateway rlpx_gateway_classifies_eth71_bal_messages_as_supported_sync -- --nocapture` (`0x22/0x23` treated as BAL only after negotiated `eth/71`, avoiding eth/70 snap offset collision)
- native RLPx current-geth capability floor gate: `cargo test -p novovm-network negotiate_eth_native_caps_rejects_pre_geth_current_versions -- --nocapture` (default public RLPx profile advertises current geth-compatible `eth/69,70,71` only; legacy `eth/66-68` peers no longer negotiate into incompatible Status semantics, and pristine Status/capability mismatches enter immediate decode-failure lifecycle rejection)
- v3 RLPx snap AccountRange gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_account_range_gate_v3 -- --nocapture` (eth/70+snap/1 uses global code `0x22/0x23` as `GetAccountRange/AccountRange`, sends State-phase state-root request and records matched response)
- v3 RLPx snap AccountRange cursor gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_account_range_continuation_gate_v3 -- --nocapture` (State phase starts `GetAccountRange` at account-hash origin `0x00..00`; non-empty `AccountRange` advances the next request origin to `last_account_hash + 1`, after storage/code/root-trie follow-ups finish)
- v3 RLPx snap account/storage/code/root-trie gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_account_to_storage_code_gate_v3 -- --nocapture` (non-empty `AccountRange` slim account => follow-up `GetStorageRanges` + `GetByteCodes` + state/storage-root `GetTrieNodes`; matched responses populate native snap account/storage/code/trie-node cache, bytecode is codeHash-checked, and `TrieNodes` responses are matched by geth-style ordered node hash so partial responses cache proven nodes and bounded-retry missing pathsets before continuation)
- v3 RLPx snap proof/root subset gate: `cargo test -p novovm-network rlpx_snap_range_proof_semantics_match_geth_complete_storage_v1 -- --nocapture` (non-empty `AccountRange` without proof is rejected; `AccountRange`/`StorageRanges` responses enforce geth-style strict key monotonicity, no deletion values, account origin/limit bounds, and slim account decode before cache; proof nodes must be valid trie RLP and include the requested stateRoot/storageRoot root node hash; when the proof resolves returned account/slot leaf values, those values must match the snap response before native cache write; if proof resolves an account at the requested origin, the response must not skip it; empty `AccountRange`/`StorageRanges` proof must prove there are no right-side trie entries before completing the range; `StorageRanges` without proof must rebuild the exact account storage root before native snap cache, matching geth's complete-range no-proof path; this is still not full multi-branch snap heal)
- v3 RLPx snap service sidecars gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_service_sidecars_gate_v3 -- --nocapture` (eth/70+snap/1 inbound `GetStorageRanges`/`GetByteCodes`/`GetTrieNodes` => protocol-valid empty `StorageRanges`/`ByteCodes`/`TrieNodes` responses)
- node native Ethereum RLPx sync entry: `NOVOVM_NODE_MODE=eth_rlpx_sync cargo run -p novovm-node --bin novovm-node` (direct Rust node entry; uses `NOVOVM_ETH_RLPX_ENODES`, `NOVOVM_ETH_RLPX_CHAIN_ID`, `NOVOVM_ETH_RLPX_TICKS`, `NOVOVM_ETH_RLPX_RECV_BUDGET`, `NOVOVM_ETH_RLPX_CANDIDATE_PEERS`; default candidate order is explicit ENODEs first, then geth DNS discovery `all.mainnet.ethdisco.net` + discv4-discovered peers, with Ethereum mainnet geth bootnodes only as direct-connect fallback; active peers stay bounded by `NOVOVM_ETH_RLPX_MAX_PEERS`)
- node RLPx public sync batch gate: `cargo test -p novovm-node eth_rlpx_public_sync_batch_defaults_are_conservative_v1 -- --nocapture` (`novovm-node` public RLPx entry defaults to conservative `NOVOVM_ETH_RLPX_HEADERS_BATCH=128` and `NOVOVM_ETH_RLPX_BODIES_BATCH=64`, reducing large frame pressure on churny public peers without changing the lower-level native fullnode defaults)
- node RLPx stalled peer refresh gate: `cargo test -p novovm-node eth_rlpx_peer_refresh_plan -- --nocapture` and `cargo test -p novovm-node eth_peer_endpoint_refresh_merge_does_not_shrink_pool_v1 -- --nocapture` (when public peers churn and no ready peer is available while `highest > current`, or startup has no ready peer and no remote highest yet, the product entry expands/refreshes candidates by `NOVOVM_ETH_RLPX_STALLED_REFRESH_INTERVAL_TICKS`; refreshed endpoints are merged with the existing pool so refresh does not shrink candidates; initial candidates now accept up to 512 and adaptive candidates up to 1024, while active peers remain capped by `NOVOVM_ETH_RLPX_MAX_PEERS`)
- node RLPx native restore phase gate: `cargo test -p novovm-node eth_rlpx_native_ -- --nocapture` (latest head/history stores restore validated header/body/receipt snapshots and resume runtime head at `State` when receipts are available, instead of falling back to `Headers`; latest head store also persists/restores bounded snap account/storage/code/trie-node subsets and snap AccountRange cursor progress for the restored head `stateRoot`)
- node RLPx snap cursor resume gate: `cargo test -p novovm-network native_state_sync_request_resumes_snap_account_range_from_runtime_progress -- --nocapture` (restored snap AccountRange progress feeds the next State-phase `GetAccountRange` origin, avoiding a product restart from blindly rescanning `0x00..00`)
- node RLPx current-head material match gate: `cargo test -p novovm-node eth_rlpx_native_body_and_receipt_match_current_header_v1 -- --nocapture` (`eth_rlpx_sync` tick output and native stores only treat body/receipt as available when number/hash match the current header, avoiding stale previous-block body visibility while headers advance)
- node Ethereum DNS signed tree gate: `cargo test -p novovm-node eth_dns_discovery_root_signature_verifies_geth_vector_v1 -- --nocapture` and `cargo test -p novovm-node eth_dns_discovery_entry_hash_matches_geth_tree_vector_v1 -- --nocapture` (default mainnet DNS root is a signed `enrtree://` URL; root signatures and child TXT `Keccak256(record)` hash prefixes are verified before ENR candidates enter the RLPx pool)
- node Ethereum DNS startup budget gate: `cargo test -p novovm-node eth_dns_discovery_default_max_queries_is_startup_bounded_v1 -- --nocapture` (`NOVOVM_ETH_DNS_DISCOVERY_TOTAL_TIMEOUT_MS` bounds DNS tree walk startup time, and default `NOVOVM_ETH_DNS_DISCOVERY_MAX_QUERIES` scales with requested candidates instead of scanning 512 records)
- node Ethereum peer discovery total budget gate: `cargo test -p novovm-node eth_rlpx_peer_discovery_deadline_caps_phase_timeout_v1 -- --nocapture` (`NOVOVM_ETH_RLPX_PEER_DISCOVERY_TOTAL_TIMEOUT_MS`, default 30s, caps DNS+discv4 startup/refresh discovery before the product entry falls back to already discovered endpoints plus geth bootnodes and enters RLPx ticks)
- node Ethereum discv4 multi-target lookup gate: `cargo test -p novovm-node eth_discv4_findnode_continues_when_lookup_adds_candidates_v1 -- --nocapture` (`NOVOVM_ETH_DISCV4_DISCOVERY_LOOKUPS_PER_BOOTNODE`, default 4, uses fresh random FindNode targets per bonded bootnode and continues while candidates are added)
- v3 RLPx new block gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_new_block_gate_v3 -- --nocapture` (real non-empty `NewBlock` => transaction trie root validation + native header/body import + follow-up `GetReceipts` + receipts trie root validation + native receipt snapshot)
- v3 RLPx new block hashes gate: `cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_new_block_hashes_gate_v3 -- --nocapture` (real `NewBlockHashes` announcement => peer head update + follow-up `GetBlockHeaders`)

Manual local run:

```powershell
cargo run -p novovm-node --bin supervm-mainline-nightly-gate
```

Duty report generator (from nightly artifacts):

```powershell
cargo run -p novovm-node --bin supervm-mainline-duty-report
```

Optional overrides:

- `NOVOVM_MAINLINE_NIGHTLY_RUN_MAINLINE_GATE=true|false`
- `NOVOVM_MAINLINE_NIGHTLY_SOAK_PROFILES=6h,24h`
- `NOVOVM_MAINLINE_NIGHTLY_SOAK_CHAIN_ID=<chain_id>`
- `NOVOVM_MAINLINE_NIGHTLY_SOAK_6H_DURATION_SECONDS=<seconds>`
- `NOVOVM_MAINLINE_NIGHTLY_SOAK_24H_DURATION_SECONDS=<seconds>`
- `NOVOVM_MAINLINE_NIGHTLY_SOAK_6H_INTERVAL_SECONDS=<seconds>`
- `NOVOVM_MAINLINE_NIGHTLY_SOAK_24H_INTERVAL_SECONDS=<seconds>`
