# Seal CLI preflight fixture repair

Input: `2bd5890763670627d8e452f74f663d92e75cb291`, clean main and matching
origin/main. Windows amd64, Rust 1.94.0, release, locked/offline, four build jobs,
incremental disabled. Only the existing CLI test and delivery records changed;
no production startup, UCA, AOEM, consensus or economic code changed.

## Confirmed failure

Remote CI [40094423953](https://github.com/novovm/supervm/actions/runs/40094423953)
completed with failure. Security, format and strict Clippy passed. Its historical
Mainline Gate Runner failed at
`native_seal_service_cli_bad_config_fails_before_startup_recovery` (3 passed,
1 failed); later gates did not thereby pass. Original failed-job output is retained
in `ci-input-failure.log`.

`before.log` independently reproduces the same failure on the current Windows
release build: the expected generic `config` text was not reached. The program
instead rejected an unconfigured native tick before entering the retired Seal
loader. This is a missed test precondition, not evidence of a UCA state failure.

## Existing call chain and minimal repair

- Original `novovm-node` main first calls
  `validate_original_native_tick_selection_v1`. A configured current native owner
  explicitly rejects old `NOVOVM_NATIVE_SEAL_ENABLED/CONFIG` and overlay settings,
  before reading the configuration. Supplying a valid ResidentConfig must NOT
  make the old Seal owner legal; no such coexistence was enabled.
- The historical tick path requires `NOVOVM_JULY_HISTORICAL_FIXTURE=1`. Only there
  does `NovNativeSealServiceConfigV1::load` precede the old startup recovery.
- The repaired test explicitly opts into that fixture, retaining its original
  name used by the gate. Missing-file and malformed-JSON cases must reach the
  precise loader errors `resolve native seal service config` and
  `invalid native seal service config JSON`, not a generic uppercase match.
- Separate negative tests retain refusal without the fixture opt-in and verify
  current product/old-Seal mutual exclusion for both tick/pipeline aliases and
  both old Seal environment settings. The product test deliberately uses an
  unread native config path: it proves exclusion before loading, not validity
  of a supplied production configuration.
- Existing environment isolation remains. Before/after input-path and byte
  snapshots must match; newly created directories are rejected. Both output
  streams must lack Seal startup, execution recovery, ledger recovery and native
  tick startup markers. This is sandbox-visible state plus control-flow evidence,
  not a system-wide trace proving that every process performed zero I/O.

There is no new listener, runtime or ledger. No assertion, quorum, timeout,
signature policy or production guard was relaxed. Historical fixture opt-in is
test-local and is NOT a launcher default.

## Local results

| Evidence | Result |
| --- | --- |
| `before.log` | Reproduced 3 passed / 1 failed, 0 ignored, 0.16 s |
| `after.log` | Revised real-binary suite 6 passed / 0 failed / 0 ignored, 0.05 s |
| `seal-regression.log` | Historical Seal library regressions 217 passed, 0 failed/ignored, 67.06 s |
| `write-inventory.log` | Original persistence sidecar inventory 1 passed, 0 failed/ignored |
| `clippy.log` | Targeted release Clippy with `-D warnings` passed |

Workspace formatting check (`cargo fmt --all -- --check`) also passed.

The library regressions use the existing historical comparison environment
`NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=1`; the CLI helper removes all inherited
NOVOVM/AOEM variables before setting each case's explicit inputs.

Main executable SHA-256:
`3f6b747b8d8ec95bc3c446db67a94a83e9e9e4a86aa1a88d725ca6c42b857e9b`.
Revised integration-test executable SHA-256:
`2169a230a92660e8204120594e97804f87f132a5e00a1cc35fb3713e288d20ac`.
No runtime library was updated. No TPS or new UCA asset availability is claimed.

```powershell
cargo test -p novovm-node --test native_seal_service_cli --release --locked --offline
cargo test -p novovm-node --lib native_block_seal --release --locked --offline -- --test-threads=1
cargo test -p novovm-node --lib native_seal_service_write_inventory --release --locked --offline
cargo clippy -p novovm-node --test native_seal_service_cli --release --locked --offline -- -D warnings
cargo fmt --all -- --check
```

The entire mainline runner/dual-node workflow is rerun by the new commit's CI,
not claimed as locally completed by these narrower tests. Full remote green,
Linux results, native NUAS ingress/query/catch-up and formal launch remain
separate acceptance boundaries.
