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
