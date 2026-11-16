# A: latest AOEM candidate preparation

User authorizes latest fixed AOEM build, compatibility qualification, then A existing-state run; do not repeat old-version tests or require resolution of old crashes before mainline development.

## Actual state at Beijing 2026-11-17 02:30

- AOEM main fast-forwarded to fixed source `57090b6b430c3e8f3389186ec2e97cf370a6963a`; no tracked source diff. One pre-existing untracked Python bytecode file remains, recorded by the original packager rather than hidden or removed.
- Reuse `scripts/build_aoem_ffi_fullmax_bundle.ps1` and `scripts/finalize_fullmax_bundle.py`; no new execution engine or diagnostic route. Original FullMax core/features and sidecars remain as defined by that source. GPU ML-DSA is not silently added.
- Original packager `--prepare` passed, including its real Vulkan1.2/1.3 BF16 shader compiler checks. This is NOT DLL build, package qualification, compatibility or deployment success.
- Rust stable 1.100.0, LLVM23.1.5 available. Build/preflight materials remain under ignored `target/aoem-latest-57090b6b-20261117`.
- **Build blocked by disk capacity:** D free ~2.48GiB. No full build started. Old inactive AOEM `cargo-target-ci-local/debug/incremental` measured ~38.19GiB, no compiler process observed. Agent cleanup was rejected by execution policy; nothing removed. User was given the exact native PowerShell removal command. Do not retry/bypass rejection.

After user frees disk, run the original Windows FullMax builder from AOEM using its existing Windows release cache, a fresh output directory, no `-Clean`, no Linux/package-wide CI detour. Record source stability, actual feature set, compiler, complete manifest/checksums, DLL/header/sidecars. Original packager validates native exports and unified compute smoke; do not call its GPU/proof coverage complete.

Then use the original NOVOVM runtime verification owner (`novovmctl native-identity qualify-runtime`) against this exact library and strict manifest, and the existing original-state/integer/persistence compatibility checks with isolated material. Do not relabel old package tests as new-library results. Do not rewrite an old release manifest or disable required capabilities. If a genuine ABI/state incompatibility appears, report/fix that actual blocker before opening the original A database.

Only once current A process has ended and new compatibility checks pass: launch one A LocalService `existing` instance with the new fixed package, original identities, independent log directory, and no parallel database owner. Keep B/C/D untouched. Retain old candidate only for meaningful recovery; no repeat old comparison, no new genesis. A new library running without failure does not establish C hardware failure or identify the historical corruptor.

## Old comparison, separate result

A PID21480 began Beijing02:17:44, original a254 EXE plus old AOEM `1b764a36...acdce50`, NOT AOEM966. Runtime4480s; task hard4500s, latest ~03:32:44. At02:29:59 it remained alive, no exit.json, no stderr, WHEA query completed with no events since start. This is interim only.

Do not restart it. When ended, read actual exit.json, scheduled task result, final stdout/progress, and WHEA from actual start to termination. Preserve failed/natural/time-bound/forced outcomes distinctly. No failure at a short observation interval is not stability acceptance. Existing startup receipt is under `../novonet-single-relay-20261111/candidate-a25452f4/A-existing-comparison-20261117/`.
