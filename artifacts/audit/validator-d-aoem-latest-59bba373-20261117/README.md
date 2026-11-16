# Validator D: latest source synchronized and AOEM built

User requested synchronization with A/C and compilation of latest AOEM.
Both repositories were clean and fast-forwarded on main:

- SUPERVM: bf8271ba (before this receipt).
- AOEM: 59bba373bacbc1af056fa0b7d3df35fc7cb84ca6.
- AOEM source remained unchanged and clean throughout the official build.

Official Windows entrypoint: scripts/build_aoem_ffi_fullmax_bundle.ps1,
existing release cache, fresh output, no Clean, no alternate build pipeline.
Rust1.100.0 was installed and selected for this invocation using
RUSTUP_TOOLCHAIN; the system default toolchain was not changed. Clang21.1.8,
Python3.12.6; exact shader compiler versions are in build-tools.json.

Build exited0 after562.45seconds at2026-11-17 04:19:34 Beijing.
Local bundle (NOT installed):
`D:\WEB3_AI\AOEM\artifacts\ffi-bundles\validator-d-59bba373-20261117\windows\20261117-041012`.
Core49146368bytes, SHA256
`e7f25d47a61a16820d54e4cebc30818eafd93e65cb2c7a46d72a425df4e9ecce`.

## Actual Validation

- Original packager: real shader compiler preflight, clean source identity,
  native DLL load,148 public header symbols,14 plugin files, C syntax and
  unified compute smoke passed. Independent original --verify passed the
  complete checksum inventory. Six package guard regression cases passed.
- Existing pinned a254 novovmctl native-identity qualify-runtime exited0
  against this exact new DLL, with AOEM_DLL_MANIFEST_REQUIRED=1. Scalar,
  unified semantic and mixed-batch ML-DSA-65 verification KATs passed.
  The qualification manifest adapts the existing a254 host core contract to
  this explicit library/hash; original required capabilities were preserved.
  It is qualification metadata, not a replacement installed release manifest.
- No keys/custody access, original database open, node startup/restart, deployed
  DLL replacement, service operation or multi-host object send occurred.
  No full GPU/proof workload, cold recovery, performance or mainnet acceptance
  is claimed. Current loaded D runtime was not qualified/replaced this round.

## A/C Alignment Boundary

A's latest published report uses source57090b6b and coref9e8ded2...fcaa.
D followed latest remote59bba373, one subsequent test-research commit; its
three-file diff adds test-only research code. Source history is synchronized,
but D's full build hash is DIFFERENT and must not be labeled A's same artifact.
Compiler/environment also differ (A documented LLVM23.1.5, D clang21.1.8).
This receipt makes no reproducible-byte or current C-loaded-version claim.
Coordinated runtime rollout requires explicit actual package and loaded-module
identity checks; compilation alone does not perform that rollout.

Original manifests/checksums, build source/tool identity, qualifier raw output,
exit records and regression logs are copied byte-for-byte and listed in
SHA256.json. Public Git contains NO compiled DLL, build cache or runtime DB.
Complete build stdout/stderr remain local under
`D:\WEB3_AI\SUPERVM\target\d-aoem-latest-20261117`.
