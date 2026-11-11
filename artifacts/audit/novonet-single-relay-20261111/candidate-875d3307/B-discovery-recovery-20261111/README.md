# B discovery bootstrap: restore the original owner after bounded run expiry

At20:37 Beijing, SSH read-only inspection found no novovm-node process and no
41050/42050 listener. Original systemd journal shows the same invocation
7c6e74309ab24ae99ae9f3a6a5aa98aa started Nov10 17:31:50.066426 and deactivated
successfully Nov11 17:31:52.260026. No current crash was observed.

Original8fed updater launch explicitly set NOVOVM_NATIVE_EXECUTION_RUN_MS=86400000
and RuntimeMaxSec=86520. Native tick exits when that process run duration elapses.
This explains the current stopped bootstrap and resulting inability to connect;
it does not retrospectively explain failures while B was still running.

## Minimal actual recovery

The original scripts/linux/update-novonet-existing.py now has resume-current:
verify the installed pinned package, original owner/config/database/custody,
absence of process/listeners, exact matching systemd clean-stop pair and last
progress; start the same program/config with existing and separate new logs.
Explicit continuous mode omits the24-hour process duration and systemd runtime
cap. Default bounded upgrade behavior remains unchanged. No connection timeout,
authentication, resource limit, network path, cache, key or ledger rule changed.

The original8fed generation script produced a separate pinned updater. Uploaded
and locally regenerated file SHA256:
95027fcbfdfcbac81d38bb75e8106e2de38598ae05f227e8d21afa3e65c374dc.
Original source tests16/16 and regenerated8fed tests16/16 passed under local WSL
Linux. No new node build, candidate Full, initializer or installed package swap.

## Verified live results

- Unit novovm-B-8fed4b76-network-20261110-continuous.service, PID3766709,
  original novovm account, active/running.
- Same8fed program hash93394930458df988b789cab036bebb18bb77e6babf028f179b9a62e82e108f07.
- SameAOEM hashd73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33.
- Original database /var/lib/novovm/selfrun/91129655606029/B/chain-state,
  original custody hashes unchanged, current_height1/headnull.
- UDP41050 and42050 owned by the exact new PID.
- A875 client authenticated B Status at12:41:58.2532071UTC:74ms, height0,
  current_height1, not recovering, no projection failure, original state root
  f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333.
- At12:43UTC process environment has no RUN_MS; RuntimeMaxUSec=infinity.
  B inbound Tls Ready and Stream Ready observed; failure first/lastnull.

This is an ongoing transient original node service, not a new network daemon;
boot persistence and crash restart are not claimed. Stop it via its exact unit
if needed. Old logs/config/program remain; no unrelated service was restarted.

B-original-public.zip contains9 selected public originals, including updater,
manager-stop evidence, custody digests (not key material), health and observation.
Member hashes were reread and verified. Whole run configs/database/keys omitted.

## Remaining live acceptance

A's automatic reacquisition of C/D locators and the one A->C->D object still
require actual evidence. At this receipt A privileged read-only observation is
waiting for UAC. C/D should refresh existing collector liveness, not restart
nodes or automatically re-arm expired collectors. Successful B Status or TLS
alone is not NODE_GET, relay forwarding or Object DONE acceptance.
