# Double-relay candidate: local rustix security increment

Base:1baa6dbcb9159b53abab3980b7a034bc9ea6c12b. No runtime node changed.

The original Windows/Linux packagers completed assembly of1baa6dbc. Before
package qualification or any Full dispatch, the unchanged root Cargo Deny gate
failed on [RUSTSEC-2026-0486](https://rustsec.org/advisories/RUSTSEC-2026-0486.html).
The affected1.1.4 xattr Buffer API could mark unwritten bytes initialized for a
zero-length buffer. The advisory identifies1.1.6 as patched; the0.38 line is
unaffected. This is not evidence that C's875 heap crash has this cause.

## Exact change and reachability

- `novovm-network` directly uses rustix only for Android nonblocking `flock` in
  `product_service_file.rs`; no direct project xattr call was found in the
  inspected network/storage sources. This is not proof of universal unreachability.
- The root reverse dependency tree also contains `tempfile`. Only rustix1.1.4
  is updated to1.1.6 by Cargo; unaffected rustix0.38.44 remains via atomicwrites.
- Raise the existing Android manifest floor from1.1 to1.1.6, retaining `fs`.
  Root Cargo.lock changes only that package version/checksum and two references.
- The nested runtime workspace contains no rustix; its lockfile is unchanged.
- No xattr adapter, database migration, quota logic, network policy, AOEM,
  authentication, consensus, production timeout or relay limit is changed here.

## Actual local results

| Check | Windows | Linux |
| --- | --- | --- |
| Original protected file/compaction/crash-boundary recovery |7/7|7/7|
| Original relay route/replay/shared-budget rejection |3/3|3/3|
| Original storage-budget/WAL/atomic file recovery |12/12|10/10|
| Strict Clippy: network, storage-budget, node, ctl |4/4|4/4|

No ignored tests in these results. Platform counts differ because the original
storage suite has Windows-specific cases; counts are checked, not inferred.
These are local component tests, not Android or four-host network acceptance.

Root and runtime `cargo audit` and `cargo deny check advisories bans licenses
sources` all exit0 after the patch. Existing warnings (including rand's already
configured advisory exception and a yanked crypto-bigint warning) remain; no
policy file or exception was changed. Initial Audit exit0 did not mean clean:
the rustix advisory was reported as an informational unsound warning, whereas
Deny rejected it. Therefore both gates matter.

One post-patch advisory fetch failed before analysis. That failure is retained;
using the existing127.0.0.1:7897 proxy only in the child command environment
allowed the four checks to complete. No global proxy/network setting changed.

See [source hashes, exact commands and results](results.json) and
[raw old/new logs and validation scripts](local-evidence.zip).

## Remaining candidate work

Freeze the corrected source and rebuild with the original packager; independently
verify all archive members, actual Node/Client/ctl/AOEM hashes, packaged identity,
discovery/object owner and existing recovery gates. Only then dispatch one Full
for that fixed candidate. Prior1baa assembly is not qualification for a new SHA.
No Full success, fixed new archive, deployment or real double-relay DONE is claimed
by this record. Preserve the existing real single-relay success without repeating it.
