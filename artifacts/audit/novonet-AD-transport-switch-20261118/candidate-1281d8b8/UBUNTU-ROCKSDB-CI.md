# Linux Full build compatibility and Windows package reuse

The previous [Ubuntu native ML-DSA job](https://github.com/novovm/supervm/actions/runs/44343036249/job/134938300971)
failed before the authentication executable ran. Its Ubuntu 26 compiler
reported missing fixed-width integer declarations in the pinned bundled
RocksDB 8.10.0, identifying `<cstdint>`. The native qualification job now
sets `CXXFLAGS_x86_64_unknown_linux_gnu=-include cstdint` only on its Linux
leg, before the original locked cargo build and tests. The same Ubuntu
and Windows runners, pinned Rust, runtime artifacts, security policy,
required tests and Full aggregate remain in force. No storage crate,
storage format, chain behavior, or Windows build input changed.

Local verification: 29 workflow contract tests passed. A WSL Ubuntu 24.04
check executed the workflow step's environment assignment and compiled
the original bundled blob metadata, trace record, trace result, and write
batch translation units with the resulting flag (GCC 13.3; syntax-only).
This is not a local claim of running the full Ubuntu 26 suite or running
four physical hosts. The subsequent exact Full must qualify Ubuntu 26.

## D package is the existing fixed Windows archive

The archive and the product source commit remain unchanged: `1281d8b8`.
The strict diff allowlist from that source contains only this workflow
change, its CI regression and audit evidence. No Windows source/build,
Cargo manifest/lockfile, packaging, runtime, or AOEM input changed. A
re-read all 91 payloads inside the 94-entry archive and independently
matched ZIP, manifest, Node and AOEM hashes. Reuse the original archive;
do not relabel the archive's embedded source to the CI-only commit.
See [path, size and hash](windows-archive.json) and
[package-context tests](windows-qualification.json).

Copy and verify only until the new Full for the CI-only revision succeeds
and its product-input equivalence is checked. Thereafter update only D's
original LocalService owner in `existing` mode, preserve the original
database/keys and exact listener scope, and return one fresh health receipt.
A must authenticate D and coordinate one new Iroh-only sample with timely
D fragment capture. No B outage, old object resend or native return repeat.
