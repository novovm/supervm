# C source investigation: first bounded isolation, 2026-11-12

Owner: validator-C, per latest user instruction. Ownership notice was pushed
as abde6900; receipt by A has not been established. This is diagnostic progress,
not a product fix or candidate qualification.

## Evidence

- Exact original875 PDB and original dumps remain the fault evidence. The
  exception-context registers agree with the context used for offline analysis;
  PID3228's ThreadList context is a later exception-handler context and is not
  substituted for its ExceptionStream context.
- `profile.rs` and `service/rpc.rs` have no source diff between875d3307 and
  the reviewed2ec9ff47. `status` calls `profile::snapshot`, whose `hash`
  creates per-byte formatted strings. That code shape is consistent with the
  symbolized allocating-format/String collection frames; it does not establish
  that the formatting function caused the invalid descriptor.
- The snapshot/buffer path is safe Rust using Mutex/atomics. Inspection alone
  does not exclude undefined behavior earlier in native dependencies or a
  different cause. No source-level corrupting operation has been established.

## Local isolation result

Exit0, release optimization, rustc1.94.0 / LLVM21.1.8 on Windows x86_64 MSVC.
Four writers attempted80000 original delivery-event calls; two readers plus a
final read checked501 snapshots and1014272 hash fields, including JSON round
trip, bounded event count, expected lowercase hex length and repeated-byte
integrity. Runtime2287ms; build plus run about8s. No crash reproduced.

`run_isolation.py` reads the original source with an exact SHA256 gate. It
removes ONLY the unused sibling `traffic` module declaration to avoid linking
the network. All compiled snapshot/hash/buffer functions retain their source.
This is a `cargo run`, so cfg(test) legacy integer code and existing unit tests
are not compiled/executed. Test input is explicitly diagnostic fixture data,
never a node observation or service/payment claim.

Direct serde_json1.0.149 (preserve_order/raw_value), indexmap2.14.2 and
sha2 0.10.9 match workspace versions. `isolation-Cargo.lock` records resolved
transitive versions; the isolated build is NOT the original875 binary or an
exact full-node link/compiler replay. No AOEM/FFI, node runtime, sockets, keys,
database, observer or object submission was exercised. Failure to reproduce
does not clear that code or prove that FFI is responsible.

Raw execution stdout/stderr and exit/times are adjacent. Generated executable,
source copy, dumps and raw memory remain local under ignored target. Source
checks must precede reruns; this invocation was a single bounded local run.

## Remaining work

C continues from the original invalid frame/descriptor to its producer and
reviews applicable unsafe/FFI lifetime and ABI boundaries, using isolated
reproduction before selecting a minimal fix. Do not patch safe hex formatting
merely to change the crash site, mark the node repaired, or deploy this harness.
A should avoid a competing fix and can supply runtime/build facts without being
the prerequisite investigator. C remains NOT READY for relay delivery.
