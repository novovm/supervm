# Original-owner bounded diagnostic release

This addresses the actual negative sample at candidate5b/live-inflight-0052:
two sender/receiver deliveries passed, but independent1s local command read
phases produced at least461ms separation. Near-simultaneous file publication
was not simultaneous original-owner admission. No old test/object is rerun.

## Implementation scope

- Original novovmctl `native-network-probe send-relay --release-at-ms` creates
  a bounded LOCAL envelope `NVPROBR1 || u64be release time || original RelaySend`.
  Only RelaySend may be enclosed, one/two original ordered relays. Maximum457
  local-file bytes; original network frames/object/identity/TTL stay unchanged.
  Old owners fail closed on unknown envelope instead of immediate dispatch.
- Original Control keeps its one async file read and1s read interval. A decoded
  schedule holds only one pending original command, with a100..5000ms future
  lead within original signed/probe validity. No thread/socket/watch service,
  extra reliable transport, timer task or faster filesystem polling.
- Reuse ClockAnchorV1 to map the local wall release once to an Instant. Original
  step polls this pending deadline before file-read gating. More than20ms late,
  monotonic reversal or a50ms wall/monotonic mapping discrepancy rejects and
  clears pending release; no late catch-up send or renewed TTL. These are bounded
  diagnostic scheduling criteria, not production message/consensus tolerances.
- Consumed cancellation/replacement clears pending release; repeated identical
  bytes do not rearm. At release the UNCHANGED original diagnostic State::command
  still validates identity/path, ready reservation, nonce, lease, cancellation,
  single-send and original message validity. Scheduled is not admitted/DONE.
- Original A/B coordinator submits these commands ahead of the common measured
  deadline instead of timing external file moves. Cross-host clock samples are
  management timing evidence only, never asset or authentication authority.
  Actual admission-to-DONE conservative overlap remains the acceptance boundary.

## Current qualification

Windows/Linux explicit release-target local validation passed: scheduled filter
5/5 (four new release tests plus one pre-existing leader test), original control
4/4 (overlaps three of those new tests), diagnostic constraints5/5 and CLI3/3.
Both strict Clippy runs passed with warnings denied; Rust format and diff checks
passed. Original management sequence38/readiness20/reader23/restore6 passed.
These groups overlap; do not sum them as independent test counts.
Not yet a Full result, qualified fixed package, runtime deployment or concurrent
real-host pass. First redundant untargeted Windows check/Linux test builds were stopped,
logs retained, to reuse existing explicit release-target caches. Initial inline
WSL quoting failure did not run tests; the explicit script invocation is used.
No existing node or cloud CI was stopped. No C/D action or new observer window
until a qualified fixed program is actually ready and the sender owners updated.
Only A/B originate the scheduled commands, so only they need this new diagnostic
owner program. C/D5b receive/forward the unchanged native network frames and can
retain their actual qualified program/identity/database; no duplicate ZIP copy or
upgrade needed merely for this local scheduling envelope. Record the mixed runtime
versions honestly in the eventual live result, never claim all four new-version.

After affected tests/strict Clippy, synchronize main and validate one fixed Full,
build/check original Windows/Linux packages, then use existing update owners.
Only missing true in-flight concurrency is retested with distinct new objects;
accepted direct/single/double/recovered objects and failed timing samples remain.
