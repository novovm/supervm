# Security dependency checkpoint — 2026-10-24

Input: SUPERVM `2d504f07f5cc8def832825f6ca995beebe0829c5`; Windows/PowerShell,
repository Rust toolchain 1.94.0. This is a dependency-resolution/audit checkpoint,
not full CI, network regression, cryptographic side-channel qualification, mobile
runtime or production-release acceptance.

## Observed failure and sources

GitHub CI `40086131757`, completed security job `120849444630`, failed at Cargo
audit. Cargo deny was skipped there. The job log was read through:

```powershell
gh api repos/novovm/supervm/actions/jobs/120849444630/logs
cargo audit --json
```

The local advisory database reported commit
`21d909e4c8bd4e14c7aea58bf980c881a0053ab1` (1,352 advisories).

| Advisory | Observed package | Patched range / treatment |
| --- | --- | --- |
| [RUSTSEC-2026-0385](https://rustsec.org/advisories/RUSTSEC-2026-0385.html) | ctutils 0.4.3 | Updated precisely to 0.4.4; root lock's optional Iroh dependency |
| [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html) | crypto-bigint 0.5.5 | **Open**; advisory requires >=0.7.6 |
| [RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html) | tokio 1.50.0 | Local Cargo deny exposed this additional fatal gate; updated to patched 1.51.6 |
| [RUSTSEC-2026-0382](https://rustsec.org/advisories/RUSTSEC-2026-0382.html) | rustls-webpki 0.103.15 | Found in independent historical NOV proof workspace; updated precisely to 0.103.16 |

Primary fixes: [ctutils](https://github.com/RustCrypto/utils/pull/1569),
[crypto-bigint](https://github.com/RustCrypto/crypto-bigint/pull/1365),
[Tokio](https://github.com/tokio-rs/tokio/pull/8587),
[webpki](https://github.com/rustls/webpki/pull/546).

## Minimal changes

- Root `Cargo.lock`: ctutils 0.4.4, tokio 1.51.6, mio 1.2.0,
  tokio-macros 2.7.2.
- `crates/novovm-network/Cargo.toml`: exact mio 1.1.1 -> 1.2.0;
  existing features preserved. Patched Tokio requires mio ^1.2.0, so the prior
  exact pin prevented resolution.
- `runtime/novovm-network/Cargo.toml`: exact tokio 1.50.0 -> 1.51.6 and
  mio 1.1.1 -> 1.2.0; existing features preserved.
- `runtime/Cargo.lock`: matching tokio/mio/macros update.
- `runtime/proofs/nov-transfer/Cargo.lock`: matching update because this
  independent workspace consumes the runtime network manifest. Resolution also
  reconciled rustls 0.23.45 -> the already-required exact 0.23.46, then patched
  rustls-webpki to 0.103.16. No proof/guest relation was changed.

No blanket dependency upgrade, new advisory exception, crypto backport, version
relabel, compiler downgrade or weakened CI command. Existing rand/paste warning
policy was not changed. Native-auth lockfiles had no affected dependency and
were not edited. Stale member-local lockfiles ignored by their containing Cargo
workspace were not regenerated. The historical RISC0-patched guest crypto fork
was not modified or newly qualified by this checkpoint.

Commands applied (failed attempts retained here rather than reported as success):

```powershell
cargo update -p ctutils --precise 0.4.4
# Initial tokio precise updates failed under the old exact mio/tokio pins.
# After the two reviewed manifest changes:
cargo update -p tokio@1.50.0 --precise 1.51.6
cargo update --manifest-path runtime/Cargo.toml -p tokio@1.50.0 --precise 1.51.6
cargo update --manifest-path runtime/proofs/nov-transfer/Cargo.toml -p tokio@1.50.0 --precise 1.51.6
cargo update --manifest-path runtime/proofs/nov-transfer/Cargo.toml -p rustls-webpki@0.103.15 --precise 0.103.16
```

## Validation actually performed

Root integration later saved `audit-final.json` from `cargo audit --no-fetch --json`:
exit 1, exactly one fatal vulnerability (`crypto-bigint`), existing rand/paste
informational warnings retained. The initial `--offline` invocation was rejected
by cargo-audit itself (exit 2; `audit-stderr.log`), not a dependency audit result.
The no-fetch result uses the already inspected advisory DB; no exception added.

`cargo metadata --manifest-path <manifest> --locked --offline --no-deps
--format-version 1` returned exit 0 for all four:

- `Cargo.toml`
- `runtime/Cargo.toml`
- `runtime/proofs/nov-transfer/Cargo.toml`
- `proofs/native-auth/Cargo.toml`

`cargo audit --file <lock> --no-fetch --json` returned exit **1** for root,
runtime and runtime/proofs/nov-transfer lockfiles. Each now reports exactly one
fatal advisory: **RUSTSEC-2026-0387, crypto-bigint 0.5.5**. ctutils, Tokio and
webpki are no longer reported in those corresponding locks. Existing rand
unsoundness / paste maintenance warnings remain; no new exceptions were added.

```powershell
cargo deny check advisories bans licenses sources
cargo deny --manifest-path runtime/Cargo.toml check advisories bans licenses sources
```

Both returned exit **1**, solely for crypto-bigint's advisory. Bans, licenses and
sources passed (duplicate/unencountered-license warnings remain). `git diff
--check` passed. Tokio, Mio and tokio-macros declare MSRV 1.71, below repository
1.94.0. These checks do not compile or execute the network.

The initial agent commands/output were observed in the development tool
transcript, without separate files. The root owner's later `audit-final.json`
is the saved raw recheck, not a recreation of the earlier output. The GitHub
job above retains the original remote failure. Root integration also passed
149 original NovoRUDP tests and 6 actual-AOEM native lifecycle tests; those
logs and their exact artifact boundary are in the adjacent UCA checkpoint.

## Remaining blocker and regression boundary

Actual root dependency path:
`crypto-bigint 0.5.5 <- elliptic-curve 0.13.8 <- k256 0.13.4`, including EVM,
network and node consumers. The preserved runtime network also uses k256 0.13.4.
There is no registry crypto-bigint 0.5.6 (`cargo info crypto-bigint@0.5.6` failed).
The prescribed >=0.7.6 is not a semver-compatible lock-only replacement for
elliptic-curve 0.13.8. Stable upstream k256 0.14.0 exists and uses the newer
elliptic-curve family; migrating it requires explicit API/behavior regression
across consumers, not an untested lock substitution. That migration was not
performed here. **Security CI must remain red until the blocker is resolved.**

Tokio patch notes include mpsc/semaphore/waker, current-thread timer and Windows
child-handle fixes; Mio 1.2.0 also adjusts listener backlog to the standard
library. Root integration must therefore run original network/reconnect/runtime
regressions, not infer behavioral acceptance from successful metadata. No TPS,
GPU, UCA finality, mobile or mainnet claim follows from these dependency edits.
