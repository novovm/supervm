# AOEM four-host version comparison and source reconciliation

Status: **read-only four-host comparison requested; four fresh host reports not yet collected.**
Do not use this document as authorization to replace a node runtime or to send a relay object.

## Source reconciliation on C

AOEM GitHub `main@9662470d` now contains the previously missing
`aoem_compute_result_take_v1` implementation and C header declaration. The
prior `main@fd5cfb31` did not contain them. The four-host a254 package used
AOEM base `4c57372ad0cc12123df5ca9e163a130039bfb599` plus a separately
identified source overlay SHA256
`8ee88c113611cbdcfacce949ae06b49f9870bb8cdd570a8eddeb8343505ab4fb`.
This is a source/artifact version gap, **not a Git branch**. C cannot inspect
A's uncommitted files. A should compare its local material with the newly
committed source before deciding whether any further merge is needed.

The FFI allocation/free correction from AOEM `fd5cfb31` is an ancestor of
`9662470d`. C also built the pinned old Fullmax source plus that five-site fix
in isolation. Its Windows DLL SHA256 is
`b3305f57213d9c5c2048d58b6928b6a495d63ff8bef1c86d530d7855e44f7690`.
The isolated ZIP SHA256 is
`9b3d32a907361419a1c40a8e60e88605399513b08e04444aecd3d16c49651367`
(83,043,145 bytes). ZIP CRC, all 26 Windows component checksums, ABI 1,
Fullmax capability probe and original SUPERVM owner isolation passed. This
**older pinned-source candidate is retained only as comparison evidence** now
that `9662470d` is on main; it has not been deployed to C or qualified across
hosts. C's old a254 node remains down. The exact root of C's 875/a254 crashes
is not established by these component tests.

After A pushed `9662470d`, C fast-forwarded AOEM main. The committed FFI source
file SHA256 `934937edf7d83a4e3b51042c2a7f65dd798f17a49e0a6555c7be62a14c29117a`
is byte-identical to C's corrected pinned FFI source file. This resolves the
specific missing-export source gap on main; the broader source and build inputs
also changed in A's commit. C built `aoem-ffi` with the old Fullmax feature set
using Rust 1.94.0 and `--locked`: **PASS**, Windows core DLL SHA256
`5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52`.
The new DLL exports `aoem_compute_result_take_v1`, `aoem_execute_ops_wire_v1`,
`aoem_state_read_v1` and `aoem_free`; ABI 1 and `execute_ops_v2` probe passed.
It passed 1,024 varied-size state-read/free cycles and the original SUPERVM
owner-isolation workload: 1,024 positive and 1,024 negative semantic verifier
calls, 32 private contexts, 501 snapshots and 80,000 writer attempts. The
first harness invocation omitted its opt-in profile variable and exited before
AOEM work; the corrected invocation passed. Raw build/probe logs remain local.
These are component checks, not complete node, sidecar or cross-host acceptance.

AOEM `9662470d` lock SHA256:
`04b9fca68787b399576de7b5bdc43cdf6c63180df4bd5331656391adbb784f66`;
public header SHA256:
`d983cf69b0f64e9a40e45d2e9c30dd1cd2cc99d1e8ddb33ed5ffeefcb2b4cd38`.

## One coordinated read-only capture for A/B/C/D

A coordinates a single Beijing-time comparison window and distributes this
Git commit to B/C/D. All four Codex instances run the corresponding collector
once in that window, into a **new** evidence directory, then return the raw
output and exit code. The script reads process/module and installed-artifact
identity only. It does not start or stop nodes, modify libraries, clocks,
databases, keys or network configuration.

Windows A/C/D, from SUPERVM root in an elevated PowerShell 7 if module access
requires it:

```powershell
$evidence = 'PATH-TO-NEW-HOST-SPECIFIC-EVIDENCE-DIRECTORY'
New-Item -ItemType Directory -Path $evidence -ErrorAction Stop | Out-Null
pwsh -NoProfile -File scripts/aoem/Collect-AoemHostVersion.ps1 -HostId C -RuntimeRoot 'ACTUAL-INSTALLED-AOEM-ROOT' -OutFile (Join-Path $evidence 'aoem-host-version.json')
$LASTEXITCODE
```

Use `-HostId A` or `D` on those hosts. Set `-RuntimeRoot` to the actual current
installed AOEM root, not a candidate staging path. If the root is unknown,
omit the parameter and report that fact; do not guess. The collector reports
loaded AOEM DLLs separately from installed files. If no node is running,
`processes: []` is a valid result, **not** proof that a staged DLL is active.

Linux B, from SUPERVM root:

```bash
mkdir -p 'NEW-HOST-SPECIFIC-EVIDENCE-DIRECTORY'
bash scripts/aoem/collect-aoem-host-version.sh B 'ACTUAL-INSTALLED-AOEM-ROOT' > 'NEW-HOST-SPECIFIC-EVIDENCE-DIRECTORY/aoem-host-version.txt'
printf 'exit_code=%s\n' "$?"
```

The Linux collector reads `/proc/<pid>/maps` and the installed manifest. Use
the actual root or omit it if unknown. If permissions prevent reading the
running process, report the failure verbatim; do not infer the loaded library
from a package file. No private keys or memory dumps belong in Git.

## Comparison rule

Each report must show host, UTC capture window, live node PID or absence,
loaded library path/hash if accessible, installed core hash, manifest hash,
declared core hash, and source identity. For Windows A/C/D, the same release
should resolve to one exact Windows DLL hash. B's Linux SO hash **will differ**;
it must match its own manifest and the same approved source/overlay, ABI,
feature set and package release identity. A/C/D/B must not mix the pinned old
candidate with new AOEM main. A compiled export alone is insufficient to
qualify the full runtime or a live node.

Historical a254 evidence reports Windows A/C/D core hash
`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`
and Linux B core hash
`d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33`.
These historical values are comparison inputs, not fresh current-state reports.
After all four reports arrive, A/C compare source identity and actual loaded
module before proposing one fixed cross-platform candidate and staged existing
updates. Comparison happens in one window; stopping all four nodes at once is
not required by this version audit.
