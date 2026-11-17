# D_COMMON95_EXISTING_READY_FOR_A_QUERY

2026-11-17 11:40:59 Beijing (03:40:59 UTC): actual D common95 owner is running.

- One approved existing update, controller exit 0; preflight exit 0. No repeated update/start.
- PID23340, created 03:39:29.7521670Z, NT AUTHORITY\LOCAL SERVICE (S-1-5-19).
- Task: NOVOVM-D-Common95-Existing-20261117, Running, no diagnostic process lifetime cap.
- Original database: C:\ProgramData\NOVOVM\selfrun\91129655606029\D\chain-state. Cold copy file hashes checked before startup. No initializer, rekey, LOCK removal or old-package restart.
- Original identity hashes unchanged; current height1/head null unchanged, recovery false, execution failures0, projection error null. This is local projection equality, not a new authenticated root query or ledger-equivalence proof.
- Listeners192.168.71.59:41053/42053 belong to PID23340. Existing firewall filter scope and global profile settings preserved; only new program association added.
- Actual node SHA256: 754eb3f50ab6111ec494d30b00af52e8dbee43038f54e0a1c313e8579bfc6bf7.
- Actual loaded AOEM SHA256: f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa.
- Release manifest SHA256: 82e5229ed041af942e0333597ce1fbfcca6f9f66f69dd4bb598d69b5c0e66ea8.
- AOEM manifest SHA256: 99bb2346ba31f812028cff4c4da9dce1209f09f979f5f399ec01cb4871c1bf38.
- Whole-package ZIP SHA256 rechecked: 39eba348fed675f853a0c4bdf60f6b4bae789fb99cc04fc89c4614a99285932c. All91 declared extracted payloads checked by length/hash and nonredirected in-package paths before update.
- Current config: C:\ProgramData\NOVOVM\selfrun\91129655606029\D\novovm-common-95ec9c2e-20261117\run.json.
- Current progress: C:\ProgramData\NOVOVM\selfrun\91129655606029\D\novovm-common-95ec9c2e-20261117\logs\progress.json; last modified03:40:55.1217334Z during final inspection, no exit/error record.

The earlier helper-hash rejection is retained separately. The D-only deployment entry now hashes bytes after CRLF-to-LF normalization and parses that same verified content. It does not ignore arbitrary whitespace or accept changed code. Tests: LF, CRLF, mixed EOL accepted; code/space/bare-CR/BOM mutations rejected; actual pinned helper accepted (8 checks). Node production startup/custody/TLS guards and binaries were not changed.

Raw controller stdout/stderr and exit records, actual startup public receipt and subsequent administrator read-only health result are adjacent. The controller's exit0 is not a node exit: the node stays running. Protected DB, keys and private config are not published. A may now perform its authenticated D query. No observer, object send, relay DONE, production/finality or performance acceptance is claimed.
