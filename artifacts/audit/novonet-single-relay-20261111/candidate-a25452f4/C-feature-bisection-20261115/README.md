# C feature-bisection baseline qualification

PT/MSR work is paused. No original node or database is used. No production fix.
Source audited: a25452f4 crates/novovm-node/src/bin/novovm-node.rs.

## Actual findings affecting experiment validity

1. In the configured product loop, periodic network.observation() and
product.status() depend on last_progress_report_at and the interval, NOT merely
the report-file path. Removing PROGRESS_REPORT_PATH does not disable status
construction. Such an F03 would be a false feature-off control.
2. Prior isolated setup already has workers=1 and
AOEM_SEMANTIC_GRAPH_V2_WORKERS=1. It still has network/runtime threads. Calling
that same setup F04 concurrency-off would be false; a verified complete
single-thread switch is not established.
3. fullmax runtime profile lists thread settings, not proof of GPU graph
execution. F01 is blocked on baseline GPU-path evidence, not assumed relevant.
4. Existing executable-specific WER points to the LocalService protected
NOVOVM-Full-C-20261114T074716Z directory (DumpType2, DumpCount2). Current-user
Test-Path was denied. Prior isolated harness runs as current user. A new long
run cannot assume its crash dump will be saved there. Do not change that rule
or ACL behind the preserved original environment.
5. Old isolated loopback workload covers real protocol turnover, but historical
packet order, remote timings and LocalService heap context were not reproduced.
BASE must be called an approximate workload baseline, not exact failure replay.

## Experiment table (no new run has started)

| ID | Combination | Window | Operations | Result |
|---|---|---|---|---|
| BASE | Original a254/AOEM966, fresh isolated state, NOVONET + periodic status | 75min bound, covers historical ~66min if reached | Pending actual ticks/transport turnover/thread/memory counts | Not run: dump-account precondition unresolved |
| F01 | GPU graph off | Pending proven baseline GPU activation/switch | Pending | Not executable yet |
| F02 | Network turnover reduction/off | Requires explicit peer/receive-owner boundary and measured load difference | Pending | Not run |
| F03 | Periodic status off | Must suppress construction, not only file writes; exact switch pending | Pending | Not run |
| F04 | Concurrency reduction | workers=1 already; runtime/network concurrency remains | Pending | Not a valid off variant yet |
| F05 | Suspect pair off/on/re-enabled | Only after reproduced BASE and discriminatory result | Pending | Not run |

BASE precedes ablation. A finite no-crash run is censored evidence, not a fix or
module exclusion. Require re-enable and interaction controls after an apparent
discriminator, comparing actual workload counts as well as elapsed time.

Next concrete prerequisite: prepare an isolated crash-output setup bound to
its actual runtime account without changing the original executable WER rule,
then run BASE without CDB/PT/TTD. This report does not claim a completed
experiment or a narrowed defective module. Original identity/database/dumps
preserved; double relay remains paused.
