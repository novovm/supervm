# Actual Full security blocker, not an eight-slot network failure

Fixed runtime-source `e666a6fb8e1f7127b27fb1bc38568e1eea1dc052` was pushed/remote-verified. One original `profile=full` dispatch was made: [44079683535](https://github.com/novovm/supervm/actions/runs/44079683535). Its actual head SHA matches; the original run is not cancelled or relabeled.

Actual [security job134026939952](https://github.com/novovm/supervm/actions/runs/44079683535/job/134026939952) failed root Cargo Audit: `libz-sys1.1.31`, `RUSTSEC-2026-0544`, patched `>=1.1.33`. Runtime Audit and Cargo Deny were not reached in that cloud job. The extracted original audit section and full local failure log/hash references are retained; see [independent result](result.json).

## Minimal lock correction completed locally

Official [RustSec source](https://github.com/RustSec/advisory-db/blob/main/crates/libz-sys/RUSTSEC-2026-0544.md) and actual Cargo metadata confirmed the version. `cargo update --offline -p libz-sys --precise 1.1.33` regenerated only that root-lock version/checksum; no manifest, Rust runtime, RocksDB feature, AOEM library, ledger or protocol change. Root lock retains this optional RocksDB zlib dependency; the actually selected Windows and Linux native-node normal/build dependency trees do not include it and remain byte-for-byte identical before/after. The independently hashed official crate archive was supplied to the stale Linux offline cache; no registry/source trust override.

Root and runtime Cargo Audit now exit0 with zero reported vulnerabilities, retaining their existing warnings. First local root Deny failed to fetch its advisory Git DB (TLS); process-local existing proxy/HTTP1.1 retry fetched it and revealed a **different real policy blocker**: `RUSTSEC-2026-0543`, chrono unmaintained. Root bans/licenses/sources passed; root advisories failed. Runtime Deny separately passed all four checks. No ignore or policy was added/relaxed. Existing allowed rand/paste/yanked warnings are not called repaired.

## Remaining scope decision

Official [chrono maintenance advisory](https://github.com/RustSec/advisory-db/blob/main/crates/chrono/RUSTSEC-2026-0543.md) has no patched version; it is an unmaintained notice, not the libz heap-overflow advisory. Current root consumers include native-node tools, governance observability, rollout policy and Windows `Iroh→netwatch→WMI`. It is not an EVM-only issue. A simple version bump cannot close it. Removing/replacing these time-format adapters and the upstream Windows feature/dependency path is additional implementation scope; an exception would instead change the safety policy. Neither is silently performed under the eight-connection candidate approval.

Recommended next work is a bounded dependency migration on those original callers, preserving timestamp formats, Windows interface observation and original networking/time/ledger contracts, followed by affected regressions/security checks and one final corrected-source Full. Request direction before that scope expansion. Do not dispatch another inevitably-red Full just to rediscover this known local blocker.

Both original e666 packagers completed assembly, but those outputs are **not Full-qualified, frozen archives or deployed candidates**. No Windows ZIP was sent to C/D, no actual node updated/restarted and no new diagnostic object sent. Once the dependency issue is resolved, use the original packager with the actual new frozen source/manifests; do not relabel e666 output as new-source provenance or use its failed Full as a pass. Original accepted single-object tests are not rerun.
