# Shared original-tick integration — 2026-10-16

Source parent `5c95c084` (mainchain input `cb4b3dd1` plus coordination-only
commits); reviewed source `3f0a95e4` in the July recovery workspace. The commit
containing this report supplies the exact shared source diff. No AOEM change.

## Actual executable and scope

- Windows, four independent original `novovm-node` processes on one machine.
- Debug node SHA256 `340ffb4216916881fde7a5c7c26b7159bbbf9354c842bc15091147e335c84d63`.
- Release build passed (2m28s), node SHA256
  `33a3acf51b8048fa50e12a1f50c76686f06f8acad9db67396577b84d7cd3d4ab`.
  This is a distinct executable; debug functional results are not its TPS result.
- Packaged AOEM DLL SHA256 `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.
- Strict manifest loading; `AOEM_SEMANTIC_GRAPH_V2_WORKERS=4`.
- Original `native_execution_tick` / UdpDrive owns the single authenticated
  UDP receive owner. SDK submission and binary queries use the same native
  session. No HTTP listener, old resident bootstrap or legacy pending executor.
- CPU and `aoem_integer_v7` use the same pipeline, persistence, round-bft journal
  and publication. The latter consumes the existing complete algebraic plan;
  these tests are not a new physical-device or business-proof attestation.

## Results

`cargo check --locked -p novovm-node --lib --bin novovm-node` and debug node
build passed. Strict lib/bin Clippy passed after a constructor-arity annotation;
the existing Cargo duplicate soak-target warning remains.

Native pipeline ordinary suite: **350 passed, 43 ignored** (5.04s).
Old controller suites no longer compiled are NOT counted as passed. Both original
service GPU-selection and independent-account budget tests were retained.
Network regressions: native owner/client lanes 3, UDP ingress 3, transaction
frame authentication 5, source pinning 7; binary query 1; structural source/socket
9 — all passed. These do not claim public-network deployment.

Actual-node `native_only_` tests:

| Backend | Cases | Entire test time | Reports |
| --- | --- | --- | --- |
| CPU callbacks | 2 passed | 6.28s | [closure](cpu-native-only.json), [minority](cpu-minority.json) |
| Complete algebraic integer plan | 2 passed | 14.98s | [closure](algebraic-native-only.json), [minority](algebraic-minority.json) |

These are small functional cases: fresh genesis, original signed transfers,
invalid/conflicting signatures, business failure and nonce/fees, all four final
receipts and balances, cold reopen, and successor transactions. Minority
acceptance cannot publish without quorum; restored quorum resumes. Whole-test
times are NOT transaction latency or TPS. Reports contain test public identities
and results only, not private keys, deployment configs or databases.

Actual binary negative startup checks: 15 mutually exclusive old seal/overlay/
raw/fixture/Host settings rejected before even reading the deliberately absent
config; all 3 retired resident mode spellings rejected. Initial shell harness
left an empty environment variable rather than removing it, so the next case
correctly rejected the previous setting; corrected harness removed each variable
and all 18 cases passed. No runtime data was opened in these negative checks.

## Limits / next checkpoint

Release build/load is separate; no new 65,536-transaction TPS claim here.
Native cross-height successor overlap, archived catch-up/offline backpressure,
actual native A/A/B combined faults, Linux and multi-machine acceptance remain
open. The historical Relay tests and later execution assets are preserved, but
their past PASS is not native-path acceptance. CI now verifies the new test
names exist before running them; remote result remains pending.
