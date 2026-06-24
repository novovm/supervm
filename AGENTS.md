# Agent Highest Law

This machine's active development project is:

```text
D:\WEB3_AI\SUPERVM
```

Hard rules:

- Only `D:\WEB3_AI\SUPERVM` may be modified for NOVOVM / SUPERVM development tasks.
- `D:\WEB3_AI\MEV` is reference-only on this machine. Do not edit, commit, push, format, test-generate files, or apply patches there unless the user explicitly asks for MEV recovery work.
- Other sibling repositories under `D:\WEB3_AI` are reference-only unless explicitly named by the user as the active project.
- Before any code edit, verify the current working directory is inside `D:\WEB3_AI\SUPERVM`.
- If the prompt mentions SUPERVM/NOVOVM/AOEM runtime/NovoRUDP/full async pipeline, treat `D:\WEB3_AI\SUPERVM` as the only writable workspace.
- If a tool invocation starts in any other workspace, stop and switch to `D:\WEB3_AI\SUPERVM` before modifying files.

Recovery note:

- On 2026-06-24, three transport-hardening commits were mistakenly applied to `D:\WEB3_AI\MEV` and then reverted on MEV main:
  - `935b660`
  - `efa71a2`
  - `77d9ec7`
- The revert commits on MEV are:
  - `bae3b57`
  - `ba61f0f`
  - `f5eea5b`
