# D 8fed interrupted existing recovery

Status: `D_8FED_EXISTING_READY_FOR_A_QUERY`.

Following main@0d31a776's latest CD-UPDATE instructions, D verified and freshly
extracted windows-interrupted-recovery-tools-v2.zip (18565 bytes, SHA256
0354764a6cc3432a6ed09b2108a00e6fc16cfafc698d52b98debe51e00e63fc9), then verified
all three tool hashes. Original fixed Windows package was reused without rebuild
or retransmission. Administrator PowerShell 7 tests exited 0, including the
fixed 77-member package, interrupted-owner and conflicting-mode gates.

Invoked the original updater exactly once with HostId D and RecoverInterrupted.
Updater SHA256: f1b3ecaa65167ef3145341882ffca792e543a0ed86a6abbe538b56be2059d254.
Exit 0 at 2026-11-11 10:50:58 Beijing. No initialization, clock adjustment,
LOCK deletion, key replacement, C/A/B operation or new Full CI run occurred.

## Actual result

- Program source: 8fed4b76f518935b30fcfb13930ccd3d7aae7381.
- Node SHA256: 021e7f8b0b41d8b72577d7394cd9c1fb9edb8388c4ae90903dca6f86a53a8a32.
- Loaded AOEM SHA256: 1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50.
- New PID 19756, LocalService SID S-1-5-19, task
  NOVOVM-D-8fed4b76-Existing-20261110.
- Original database C:\ProgramData\NOVOVM\selfrun\91129655606029\D\chain-state;
  START=existing, custody_unchanged=true, progress_state_unchanged=true.
- current_height=1, head=null, execution_failures=0,
  recovery_in_progress=false, projection_error=null.
- Original updater observed UDP 192.168.71.59:41053 and :42053 owned by 19756.
  Firewall scope copied exactly; original rule retained, profiles unchanged.

A can now query D at 192.168.71.59:41053 using the fixed new client and existing
D identity/domain. D is left running with the original owner's 24-hour run
duration. No object/probe command was issued during this update.

## Boundaries and originals

Prior task result remains 267014, prior_owner_interrupted=true;
clean_shutdown_proven=false and exit_cause_known=false. The old owner was not
restarted. The interruption cause is unresolved; successful recovery is not a
clean-drain qualification or proof that an old fault was fixed.

result.json retains authenticated_state_root_external_check_required=true,
root_cause_resolved=false, new_object_sent=false and business_receipt=false.
A-to-D authenticated query and real Object DONE matching remain pending.
Local update success is not relay, consensus, asset or performance acceptance.

The adjacent original JSON/log files are copied byte-for-byte with Git text
conversion disabled. SHA256SUMS.json binds their bytes. before.json, full
configuration, custody files, database and program binaries remain private/local.
