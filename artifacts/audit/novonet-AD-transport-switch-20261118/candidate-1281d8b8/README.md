# Windows candidate 1281d8b8 — receive and verify only

Source `1281d8b838843de120431a23daea37cc8906b68d`. Active manifests and
the root lockfile use `lettre 0.11.24` and its required `email-encoding
0.4.3`; `runtime/Cargo.lock` and gate policy are unchanged. Root/runtime
audit and deny passed locally. The first runtime audit failed to fetch
the advisory database; the same online command passed on retry. The
[local security receipt](security-local.json) preserves both results.

Copy this fixed ZIP from A to D's original package intake directory:

`C:\Users\leadb\AppData\Local\NOVOVM\candidate-common-1281d8b8-20261119\novovm-common-1281d8b8-windows-x86_64.zip`

- Size: `163079315` bytes.
- SHA256: `b79c4d280d20ab67288d34dd349727c20d24e442da0ac18e0b7da2ffde4eea55`.
- [Archive receipt](windows-archive.json): all 94 entries matched.
- [Package qualification](windows-qualification.json): 91 manifest payloads,
  packaged AOEM runtime and isolated existing-state recovery passed. The
  same-source Windows Iroh receiver time-window regression passed separately.

[Full #44343036249](https://github.com/novovm/supervm/actions/runs/44343036249)
ran at the fixed source, in parallel with Windows packaging. Its security
job passed. Its required [Ubuntu native ML-DSA job](https://github.com/novovm/supervm/actions/runs/44343036249/job/134938300971)
failed while compiling the bundled RocksDB native dependency before the
authenticated executable could be run. The job used an Ubuntu 26 image;
the compiler reports missing fixed-width integer declarations in the
bundled `librocksdb-sys 0.16.0+8.10.0` headers and specifically identifies
`<cstdint>`. This is a separate blocker even though the Windows archive
was successfully verified. No compiler workaround or gate bypass was
applied to this candidate.

D: receive and verify only. Do not install, stop or start any node while
the Full gate is not satisfied. After an eligible Full and an exact
package/host check, the only authorized live change is D using the
original database, identity and LocalService `existing` entry. Send a
fresh D health receipt; A must authenticate D and receive confirmation
that D can capture within the original 120-second diagnostic retention.
Only then does A send one new Iroh fallback object. D saves its original
acceptance and fragment records; A compares session, binding, and DONE.
No B stop, resend, or NovoRUDP return repeat.
