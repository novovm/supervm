# C isolated BASE run2: guard corrections

Two diagnostic-only fixes, no production edits:
- Retain previously observed NOVONET capability per node; a single missing optional `novonet` field is not network loss. Keep transport presence, positive traffic and read-error checks. This is capability qualification, not proof of sustained load; counters remain recorded.
- After waiting and reading task state, recheck result.json before declaring that the task ended without a result.

Directed verification replayed prior real samples (including the missing-field sample), rejected never-observed capability and zero traffic, checked terminal-result precedence and pending/failed states, and parsed all scripts. No test node execution in these checks.

New unique evidence/private directories; original a254 EXE and AOEM966; LocalService fresh test identities and databases. One authorized run, native limit74m40s, controller74m50s, no retry. WER Full2, bounded dump retention2; before/after WHEA and real process exits. Old failed run is preserved. No debugger/PT/TTD, original DB/identity access, production edits or double relay.

At publication: UAC requested; execution outcome pending. Do not call this run started or passed until actual started/result evidence exists. No non-reproduction establishes a fix. Runtime scripts/binaries stay under ignored target, outside release paths. Only guard logic, hashes and public evidence are published.
