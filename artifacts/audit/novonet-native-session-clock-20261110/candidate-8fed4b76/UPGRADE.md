# Four-node native session-clock cutover

Frozen program: `8fed4b76f518935b30fcfb13930ccd3d7aae7381`.
User explicitly authorized A/B/C/D and their native operator clients together.
This is a program update, not initialization, key rotation or a new chain.

## Gate and compatibility

The single Full run is `42776380753`, same exact source, attempt 1. Deployment
requires its necessary aggregate/candidate gates to succeed. Push Sync success
`42775027830` is not Full qualification. Read the actual final result, do not
dispatch again. Package qualification and live network acceptance stay separate.

Native profile 3 requires both peers. A profile-2 1ef node/client cannot perform
the new native handshake. During rolling upgrade mixed-version failure is
expected, never enable a downgrade or call it a recovered network. Check B with
the fixed new operator before advancing A, then C and D. Carrier fallback is
not proof of native UDP acceptance. No mobile NOVOCHAT package is changed here.

## Existing owners

`Prepare-Update.ps1` generates pinned copies of the original Linux/Windows
updaters, with new release/manifest/node/archive hashes and verified 1ef old
programs. It does not execute them. Original process barrier helper, package
guards, custody comparison and existing-state recovery are retained.

Order: B -> A -> C -> D. For each: read old PID/owner, library, config, live
progress and authenticated root; verify package; stop only that exact node;
start the original owner with START=existing; verify identity, original state,
loaded library, listeners and then an external authenticated Status. Preserve
old executable/config and failure records. Never rerun an initialize task,
delete LOCK, clear discovery-cache or mix old signer journal with new genesis.

B retains novovm/systemd custody. A/C/D retain LocalService. Windows updates
clone the existing program-specific inbound rule without expanding scope or
changing firewall profiles. The A operator also needs its existing UDP reply
rule bound to the new fixed client. `Prepare-OperatorRule.ps1` performs only that
same-scope copy; it does not stop a node. No manual system clock adjustment.

For C/D, use the new Windows ZIP plus the new update tools; the old 1ef Windows
ZIP and CD-recovery-tools-v1.zip are not this candidate. Run offline
Test-Update-Windows.ps1 -PackageRoot first. Only after A reports B/A healthy,
execute Update-Windows.ps1 -HostId C (then D) -PackageRoot ... -EvidenceRoot ...
from administrator PowerShell 7. Do not pass RecoverMissingProgress or rerun a
historical recovery command. Unexpected old state/exit stops and preserves the
evidence. Return public result/after/old-exit and exit code, not protected files,
before/config, databases or private keys.

## Failure and rollback

The original updater stops its exact failed candidate and requests the original
1ef program/config in existing mode. Verify the actual recovered process and
authenticated Status with the old matching operator; a rollback request is not
a successful recovery. Stop rollout on library/identity/state mismatch or
unexplained startup failure. Rollback also restores the previous native profile;
already updated peers will be incompatible until a coherent version is restored.

## Acceptance after all four are current

One fixed A->C probe should demonstrate first authentication without comparing
OS clocks, authenticated relative-time reference, native UDP send/receive and
matching Object ID/Session ID/nonce with receiver integrity and sender DONE.
Keep Transport ACK, Object DONE and business Receipt separate. Do not retry an
expired probe with a freshly invented offset, extend TTLs or change asset rules.
Then resume the existing single/double-relay and recovery matrix. No localhost
result, Status query or same root replaces those real-machine receipts. C clock
skew/suspend/jump and malicious-time scenarios remain separately unaccepted.
