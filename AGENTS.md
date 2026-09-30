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

- Before starting or resuming NOVOVM work, read
  `docs/NOVOVM_DELIVERY_ALIGNMENT.md` and the latest section of
  `docs/NOVOVM_PRODUCTION_READINESS_TRACKER.md`; verify the actual local and
  remote commit rather than relying on an earlier conversation snapshot.
- High-performance finalized transactions, cryptographic private assets, and
  post-quantum security remain core product delivery objectives. A runnable
  chain, component benchmark, storage-ownership gate, or green CI does not
  replace these objectives. Do not reduce scope without the user's agreement.
- Keep AOEM domain-neutral. Host business-policy ownership does not justify
  treating serial Host computation plus AOEM persistence as completed parallel
  transaction execution. Follow the delivery contract's acceptance boundaries.
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
