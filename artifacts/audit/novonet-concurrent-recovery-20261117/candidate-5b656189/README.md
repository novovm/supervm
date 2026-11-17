# Fixed eight-slot / dependency-repair candidate 5b656189

## CURRENT23:33: exact Full and packages qualified; NOT deployed

A fetched and fast-forwarded actual D receipt cae04350. D independently checked
the received archive size/SHA256; it has not extracted or run it. User reports
C/D both ready; no separate C receipt was present in the fetched main at this
check. Do not ask either host to recopy an already-matching package.

C-drive cleanup was user-performed; current free space is about20GB. The earlier
named e666 package directory still exists, so A does not claim it was deleted.
No further cleanup is needed to qualify these already-frozen artifacts.

Program source `5b656189784c854ac356aae77610c931b9f2189d` includes the approved
native ServiceConcurrencyV1 maximum-eight qualification, libz-sys repair and
chrono migration. The generic SDK default remains four, signed relay sessions
four and shared relay traffic 128KiB/minute. No authentication, timeout, AOEM,
ledger or consensus rule changed. WMI 0.18.4 and Iroh 1.3.0 are retained; the
reviewed netwatch 0.19.3 patch removes only unused WMI chrono feature activation.

Exact-source Full: https://github.com/novovm/supervm/actions/runs/44092874774
The original Full completed SUCCESS at2026-11-17T15:29:41Z, with exact head SHA
5b656189. A independently fetched its final run and job records: full-final.json
and full-final-jobs.json. All required jobs, including Rust, security, original
process recovery and Windows owners, passed. The workflow's conditional PR-only
job is recorded as skipped, not claimed executed. Older Full failures remain
failures. No rerun or new Full is required for this documentation.

## Fixed artifacts

| Platform | Archive bytes | Archive SHA256 |
| --- | ---: | --- |
| Windows | 162938236 | `618728e3f9b02c291af21e74343ac1a985d83370071d5818b840246b6c18c77e` |
| Linux | 375954281 | `73c79dd02796a22441b40527c97b3cda25bae9667007871dc8451e545a54bc02` |

Windows ZIP on A:
`D:\WorksArea\SUPERVM\target\candidate-common-5b656189-20261117\novovm-common-5b656189-windows-x86_64.zip`

Linux archive on A:
`D:\WorksArea\SUPERVM\target\aoem-latest-57090b6b-20261117\novovm-common-5b656189-aoem570-linux-x86_64.tar.gz`

Both original packagers completed with exact clean source and final HEAD checks.
All payload/metadata/archive members were checked: Windows91 payloads/94 archive
files, Linux92/95. Actual Node/Client/ctl/release-manifest/AOEM pins are in the
qualification JSON, not inferred from filenames. AOEM remains the approved570
fixed runtime: Windows `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa`,
Linux `21cf190ede73c2dc3040cf5ca51502a7bf56d1aeffbf7baeb31afdd5f71d0231`.

## Actual qualification

- Both packages passed original FullMax member verification and actual ML-DSA
  runtime verification KATs. These do not claim signing, live custody or network.
- Windows package original native-state cold-recovery test:1/1 passed.
- Linux package original selected network/Carrier/cold-recovery tests:17/17
  across seven existing filters, no zero-test selections. Original native cold
  recovery and runtime identity command exit0.
- Prior affected-source regressions remain147 per platform, strict Clippy and
  root/runtime Audit/Deny passed; see ../capacity8/time-migration/.
- The native test harnesses were compiled from6d2c3e7d, with this migration.
  The6d→5b change is solely the ASAS test and evidence; Cargo manifests/locks,
  .cargo, crates, vendor and runtime have no diff. Package programs themselves
  were newly built at5b. Actual harness hashes are retained, not old e666 tests.

Preserved build failures: first6d Windows packaging rejected a concurrent HEAD
change and was NOT qualified. Linux initially rejected its checkout/index state.
All42 reported audit files were proven to differ solely by checkout CRLF; their
original bytes were retained locally, then exact committed bytes/index restored.
No audit content/history or clean-source gate was changed. Final Linux packaging
ran on D, so C-drive cleanup is no longer a packaging prerequisite.

## Next, without repeating accepted samples

1. Full and fixed-package qualification are complete; do not rebuild or rerun.
2. Confirm this fixed version's deployment scope, then reuse original B→A→C→D
   existing update owners. Inspect real process, loaded library, original DB,
   identity/head, listeners and authentication per host, not historical PIDs.
3. Keep original data/custody/cache/journal; no initialize or old-package launch.
   On failure stop the successor instance and use the preserved compatible
   previous program/config existing if recovery is necessary, not two DB owners.
4. After actual update/health receipts, adapt only existing observer/coordinator
   version bindings; use fresh bounded READYs. Resume the previously UNSENT
   recovered-route object without resending the accepted pair or stopping B again.
5. Actual in-flight overlap remains separately unpassed. Use new unique objects
   for that missing acceptance; sender DONE, receiver integrity/time/custody and
   both relay circuit/path forwarding must be independently correlated.

C/D may copy and hash the new ZIP now; **do not update or open observers yet**.
No node, network setting, key, cache or ledger was changed in this qualification.
C's historical AV is not declared fixed by dependency migration or local tests.
