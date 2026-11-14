# Isolated LocalService dump prerequisite passed; BASE not started

Actual administrator run: UTC 2026-11-14 23:33:33–23:33:43.
Existing qualification executable ran once as S-1-5-19. Its declared unhandled
exception produced a full user dump of 11,070,930 bytes, SHA256
d1932bda9e48870e0d89f0b38d460ba412b555ddf249ef5a55e353d7cf54afb0.
Verifier exit0: 65,536 heap bytes, code bytes/page, expected two threads and
contexts, FullMemory and stable two hashes all passed. This is an isolated
dump test, not a NOVOVM failure or a feature-bisection result.

Only a new novovm-base-a254.exe WER image key was configured, DumpType2,
DumpCount2, new isolated folder. The original novovm-node.exe rule and original
directory ACL were not edited. A ReadAndExecute ACE for the current evidence
reader was added only on the new isolated dump folder; LocalService retains
inherited full access. The temporary probe task and probe registry rule were
removed. Raw dump remains private. Original controller acl_changed=false is
legacy wording; result-projection.json explicitly records the new-folder ACE.

To use this distinct WER rule without changing the original rule, BASE must
use the original byte-identical EXE under basename novovm-base-a254.exe. This
basename/environment difference must remain recorded, not called an exact
historical replay. Hash a254 and AOEM966 before execution. No BASE executable
has been launched in this round.

Next prerequisite is fresh isolated purpose keys generated under LocalService;
the prior current-user protected fixture keys cannot simply be assumed to
unseal under that account. Existing setup helper source has been located and
uses the upstream purpose-key implementation. Its prepare mode only will be
reused after checking binary provenance and access. Original keys are excluded.

Actual status-query invocation counters and GPU API counts are not supplied
by the unmodified baseline's current report. Mark them unavailable; report
file updates cannot stand in for exact function invocation counts. Original
network input history also is unavailable, so loopback turnover is an explicit
approximation. No debugger/PT, original database access, speculative repair,
or double relay. No claim yet of a reproduced BASE or narrowed fault module.
