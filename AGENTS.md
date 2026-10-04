# Agent Highest Law

The active development project is the `SUPERVM` Git repository that contains
this file.

Definitions:

- `SUPERVM_ROOT` is the canonical repository root returned by
  `git rev-parse --show-toplevel` from this file's directory.
- `WORKSPACE_ROOT` is the parent directory of `SUPERVM_ROOT`.

Hard rules:

- Only `SUPERVM_ROOT` may be modified for NOVOVM / SUPERVM development tasks.
- The sibling repository `WORKSPACE_ROOT\MEV` is reference-only on this
  machine. Do not edit, commit, push, format, test-generate files, or apply
  patches there unless the user explicitly asks for MEV recovery work.
- Other sibling repositories under `WORKSPACE_ROOT` are reference-only unless
  explicitly named by the user as the active project.
- Before any code edit, resolve the Git repository root and verify that the
  target is inside `SUPERVM_ROOT`.
- If the prompt mentions SUPERVM/NOVOVM/AOEM runtime/NovoRUDP/full async
  pipeline, treat `SUPERVM_ROOT` as the only writable workspace.
- If a tool invocation starts in any other workspace, stop and switch to
  `SUPERVM_ROOT` before modifying files.
- Never require a particular drive letter or workspace parent-directory name.

Shared product and multi-device development contract:

- User clarification 2026-10-03 supersedes the overbroad 2026-10-02 isolation
  rule. Preserve/restore `crates/novovm-node` as the actual product controller
  and `novovm-exec` as the unified AOEM facade, together with existing product
  modules and supporting directories. Isolate faulty execution paths, not the
  entire original product. Follow
  `docs/NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md` for scope and acceptance.
  The archive and uncommitted drafts remain protected: do not develop in,
  overwrite, or directly link unreviewed legacy sources. Restore original assets
  by complete Git-tree mapping, not by screening individual modules for value.
  Preserve all post-isolation code, tests, evidence and drafts before integration;
  review their product wiring separately. Do not roll
  back all later fixes or create a second product chain. Root Cargo/CI now
  cover the original product; runtime is a preserved nested component workspace,
  not the default product. Its same-named network package must be tested with
  an explicit `--manifest-path runtime/Cargo.toml`.
  The complete restoration checklist is
  `docs/NOVOVM_ISOLATION_UNDO_CHECKLIST.md`; the user approved that restoration
  on 2026-10-03. Assembly recovery does not sign off execution ownership or
  performance. Legacy Host compute/write paths are default-denied; the explicit
  `NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=1` opt-in is for historical comparison only,
  never a production launcher default. Keep readers, EVM and reviewed AOEM
  Transfer paths available. Follow the roadmap for the next ownership stage.

- Before starting or resuming NOVOVM work, read
  `docs/NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md` first, then
  `docs/NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md`,
  `docs/NOVOVM_DELIVERY_ALIGNMENT.md`, and the latest section of
  `docs/NOVOVM_PRODUCTION_READINESS_TRACKER.md`; verify the actual branch,
  working tree, local and remote commit, and current file claims rather than
  relying on an earlier conversation snapshot. The recovery roadmap is the
  current implementation priority; conflicting runtime-only rules and older
  next-step assignments are historical, not active authorization.
- High-performance finalized transactions, cryptographic private assets, and
  post-quantum security remain core product delivery objectives. A runnable
  chain, component benchmark, storage-ownership gate, or green CI does not
  replace these objectives. Do not reduce scope without the user's agreement.
- User correction 2026-10-04: `crates/novovm-node` is the product controller;
  Rust's `main()` is only its executable bootstrap, not a competing product
  architecture. Reuse the existing node -> exec -> bindings -> AOEM semantic
  dispatch, including the dedicated ZK workloads. A function residing inside
  node, or an API re-exported by exec, does not prove semantic integration.
  Do not promote the direct RISC0/portable-receipt diagnostic path into another
  product proof service. The backend-specific RPC service added in `387bf0b7`
  has been retired by user request; its old reports remain historical evidence,
  not the current integration direction. Preserve existing unified ZK/GPU code,
  the measured CPU fast path, reusable proof relations and recovery assets.
  Never substitute a fixed-profile proof for a different business relation or
  claim GPU execution merely from a workload name. Retire incorrect product
  wiring, not AOEM capabilities or the original node.
- User clarification 2026-10-04: integrate the complete, measured `a7db795`
  fast path into the original product on current main. Preserve product
  responsibilities, not the old serial control flow. Reviewed controller,
  pipeline, state, duplex transport and durable consensus mechanisms may be
  reused together as node internals; never create a second active signer,
  authority head or replacement product. Earlier behavior-preserving slices
  do not permanently freeze V3/state/receipt formats; version required changes.
  Establish the shared 65,536-transaction comparison and prove product RPC
  uses the SAME core. The roughly 10K result is a CPU-stage reference, not a
  final target. Retain AOEM unified algebraic/heterogeneous execution and GPU
  proof integration; do not gate CPU fast-path recovery on new GPU work or
  invent a separate CUDA route. Measure execution and proof gains separately.
- User confirmation 2026-10-03: converge consensus responsibilities into
  `crates/novovm-consensus` on the restored main. First move bounded, actually
  used protocol rules without changing behavior; separately integrate a whole
  versioned safety/signing/recovery contract. Do not mix native V3 decisions
  with round-bft votes or equate durable decision with completed publication.
  Preserve R2 and unified AOEM work. After integration and acceptance, remove
  superseded active implementations, entry points and misleading directions
  with a reviewed replacement map. Do not delete August-October work by date,
  unreviewed drafts, signing logs, runtime data or evidence. Follow the roadmap's
  cleanup exit conditions; do not start another wholesale isolation/backup.
- Keep AOEM domain-neutral. Host business-policy ownership does not justify
  treating serial Host computation plus AOEM persistence as completed parallel
  transaction execution. Follow the delivery contract's acceptance boundaries.
- Preserve the original algebraic and heterogeneous execution design without
  restoring old placeholders. Algebraic reordering requires explicit business
  effects and checked preconditions; failed transactions may still consume
  nonce and fees. BFT certificates and local execution readback are not business
  execution validity proofs. Component benchmarks do not sign off mainchain TPS.
- Work on `main`; do not create branches without explicit user authorization.
  Coordinate overlapping work through the delivery document's lightweight
  claim table. Preserve uncommitted work and other machines' commits; never
  force-push shared history. Push completed authorized changes promptly.
- Use repository-relative documentation links. State exactly which commit,
  product path, and test environment support a completion claim.

Recovery note:

- On 2026-06-24, three transport-hardening commits were mistakenly applied to
  the sibling `MEV` repository and then reverted on MEV main:
  - `935b660`
  - `efa71a2`
  - `77d9ec7`
- The revert commits on MEV are:
  - `bae3b57`
  - `ba61f0f`
  - `f5eea5b`
