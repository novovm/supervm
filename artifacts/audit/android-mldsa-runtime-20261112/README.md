# A: Android AOEM ML-DSA runtime input

## Scope

Latest decision: C owns Android build execution and phone verification using
its existing AOEM/NDK environment. A hands off the pinned inputs and wrapper,
retains shared implementation ownership, and returns to the double-relay task.
The executable instructions and private input ZIP digest are in the CURRENT
section of the original mobile runtime handoff. No successful Android SO is
claimed here. A's r2 build was intentionally stopped for this ownership transfer,
not adjudicated as a compiler regression; do not wait for it or run two builds.

Continue the A responsibility acknowledged at `5de4083d` for C's `fb394aa8`
handoff. Reuse the existing mobile build in
`docs/NOVOVM_MOBILE_AOEM_RUNTIME_HANDOFF.md`; no new cryptography, network,
JNI backend, asset authority or production loader is introduced.

The older Android core `5b08e48d4ed66893bdc7b65ec4c65e60d6fd67a58c6efa178989f857d798ac9a`
passed its historical scalar checks but is rejected by the current consumer's
ML-DSA batch/semantic capability gate. That rejection is not waived.

## Fixed inputs

- AOEM base: `4c57372ad0cc12123df5ca9e163a130039bfb599`.
- Existing AOEM overlay SHA256:
  `8ee88c113611cbdcfacce949ae06b49f9870bb8cdd570a8eddeb8343505ab4fb`.
- Qualified lock SHA256:
  `46625b6be8ad25c323fbba6e23263fe9f9f1b649805303ebdb89a7fe7f7abbec`.
- These are the existing Windows/Linux native65 core source inputs, **not** a
  claim that all changes in the overlay belong to the base Git commit. The
  current dirty sibling AOEM worktree is not exported or modified.
- Rust 1.94.0, NDK 28.2.13676358, API24, `aarch64-linux-android`, release,
  `--no-default-features --features mldsa`, as in the original mobile contract.
- Official Windows NDK archive: `android-ndk-r28c-windows.zip`, 748118221 bytes,
  repository SHA1 `086bba43ff2f5eb0e387b15c8278bb4e0d89ba1d`, checked before
  extraction. Source: Google's `repository2-1.xml`, package `ndk;28.2.13676358`.

This is a mobile capability build, not the desktop fullmax feature set or a
four-node deployment candidate. The four-machine `a25452f4` candidate is unchanged.

## Execution so far

- Original portable `native/mldsa_bridge.c` compiled with the pinned Android
  compiler, `-O2 -fPIC`, original headers and no algorithm changes: exit 0;
  `llvm-readelf` identifies ELF64 little-endian AArch64 relocatable code.
  This is compilation evidence, not Android execution or an AOEM shared library.
- Build wrapper PowerShell parsing passed. Wrong overlay, wrong lock, and
  existing output-directory cases all rejected with the expected reason.
- First source preparation failed: Windows `tar` rejected historical CJK
  documentation paths with `Invalid empty pathname`, then exited nonzero;
  no Rust build was attempted from that incomplete extraction. Local failed
  directory: `target/aoem-android-mldsa-20261112-r1`.
- Corrected preparation uses Git ZIP export of original Cargo/crates/config
  inputs, excluding documentation/distribution history. No source validation or
  product assertion is removed. Retry: `target/aoem-android-mldsa-20261112-r2`.
- Rust target download initially stalled at zero bytes; only that download
  process was stopped and retried with the already configured local HTTPS
  proxy. Target installation completed; no global proxy/route change.

Build entry: `scripts/build-aoem-android-runtime.ps1`. It retains each run in a
new output directory, pins source inputs and toolchain, records build exit/log,
checks ARM64 shared-library shape and original exports, and generates a
**build-only** candidate manifest. Export presence does not prove runtime
capability, known-answer validation, memory ownership or authorization.

## Required device continuation

After a successful build, C must report the actual core/header hashes and
candidate manifest. C then uses the existing `pq_preflight` against this exact
library, keeping wrong-hash/missing-library/invalid-signature rejection and the
current shared semantic verifier. Do not silently update the JNI hardcoded
runtime pin or call the old scalar-only evidence current qualification.

The original `MldsaRuntime`, `verification_owner_v1` and `from_mldsa` must then
be exercised by C's existing communication consumer. App UID/Keystore
enroll/restart/restore is a separate step; an adb-shell temporary key test is
not persistent app custody. No APK, node or SDK release manifest is changed by
this build task; no UCA authorization, transaction, phone qualification or
real-network success is claimed.
