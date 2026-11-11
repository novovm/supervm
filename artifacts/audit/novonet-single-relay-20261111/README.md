# Single-relay real-object execution preflight — NOT EXECUTED

2026-11-11, after the completed A→D native UDP object. No repeat of that
delivery, new probe command, node restart, locator modification, firewall rule,
clock change or protocol change was performed.

## Actual observations

- A: 8fed original owner PID13996, original UDP ports41051/42051;
  fresh progress at03:26:08Z, collected03:26:09Z. relay_forwarding=true,
  installed_bound_peer_routes=0, relay_circuits=[]; execution_failures=0.
- B: original8fed unit active/running, PID3133881. Current public projection
  has installed_bound_peer_routes=0 and two retained relay observations.
- Those B observations are **historical**, about12,313–12,355 seconds old at
  capture. Both authenticated an upstream route but have downstream=null,
  downstream_ready_us=null, zero forwarded frames/bytes, and stream_dropped_us.
  They are not a new attempt, a live usable circuit, or evidence of this task's
  object delivery. The failed downstream substage is not determined here.

`original-preflight.zip` preserves A's privileged read-only projection and B's
SSH public progress projection. Only public network observations are included.
Digests are in `SHA256SUMS.json`. No secret key, run config or database is included.

## Actual fixed-owner execution constraint

The existing service/network/relay.rs::fallback_path rotates direct/single/two-hop
routes on failed connection turns. service/network.rs enters that work only on
fallback/bound peer state and retains healthy native UDP priority.
The existing fixed probe command selects sender/recipient, not a forced relay.
No suitable active A relay circuit was observed. Sending immediately would not
prove a single-relay route and could merely repeat the already-passed direct case.

The existing real-IO hop regression makes its test topology by signing deliberately
unreachable coordinates inside its fixture. That is not permission to rewrite
live signed locators, and is not a four-machine result. Iroh's external relay alone
also does not prove a NOVOVM intermediate node forwarded this object.

Next requires a scoped route experiment: either explicitly authorized bounded
network-path isolation, or a diagnostic-only path constraint in the existing
owner with qualified deployment. Do not silently change production routing,
disable healthy direct paths, or implement another transport to manufacture PASS.
After an actual authenticated single-relay circuit exists, capture its matching
origin/relay/target route digest, endpoint bindings, native session, real forwarded
writes and receiver integrity acceptance with sender Object DONE. No result is
currently signed for that test.
