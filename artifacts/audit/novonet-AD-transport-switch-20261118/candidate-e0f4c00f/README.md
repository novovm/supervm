# Fixed Windows candidate e0f4c00f — checksum only, do not deploy

Source: `e0f4c00fa5b48b27a7617954bda519cd60db2a64`. The original Windows
product packager built all required binaries against the independently
pinned AOEM runtime. Local checks passed: all 91 manifest payloads,
all 94 ZIP members, package AOEM verification, and one isolated original
database recovery test with the packaged executable. This is not the
previous A/D live fallback or NovoRUDP return test.

Copy from A, if needed for inspection only:

`C:\Users\leadb\AppData\Local\NOVOVM\candidate-common-e0f4c00f-20261118\novovm-common-e0f4c00f-windows-x86_64.zip`

- Bytes: `163090644`
- ZIP SHA256: `66ee0598337deb83fdadf467f3aa444f82cb72c9f1f74bc61d112a71dbab6426`
- Node SHA256: `3b93f24447130e1b8be7e4059720cf0c50ed7992f15ab078b5d662b53cba8d50`
- Manifest SHA256: `50317d9120a18ff73a2a8113f4a1e99f5cda9d7a2b97ef439ec808bac992281a`
- AOEM SHA256: `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa`

The public receipts are [archive](windows-archive.json) and
[package qualification](windows-qualification.json). This does **not**
certify the exact Full run. [Rust security checks](https://github.com/novovm/supervm/actions/runs/44335457388/job/134911181792)
failed in root `cargo audit`: locked `lettre 0.11.22` has six advisories,
with a patched minimum of `0.11.24` in the job log. The root audit failed
before the later runtime audit and `cargo deny`, so those are not passes.
Keep D on its current version; do not install or start this candidate.
No node was stopped or changed, and no network objects were sent.
Do not rerun the unchanged candidate or bypass a security gate.
