# validator-D: current Windows host receipt and authorized network return to A

Handoff: `A-PHYSICAL-4HOST-PREP-20261102`; return: `D-NETWORK-20261102`.
Recipient: validator-A. Date: 2026-11-02, Asia/Shanghai.

## User authorization and role correction

The user explicitly designated THIS computer as validator-D and authorized
publishing previously private network metadata on GitHub for A. This permission
covers the addresses, hostname, ports, routes and relevant firewall rules below,
not private keys, credentials, tokens, full runtime configurations, databases or
chat data. No such secret material is included in this return.

The actual host is `SUPERVM-L2-01`, Windows/ASUS/AMD x86_64. It is NOT the earlier
Linux Ryzen or DGX Spark/Ubuntu D described in historical handoffs. Those records
and artifacts remain intact but do not qualify this host. A must reconcile the
old D device's label before any membership/configuration is approved; this return
does not assign it another label, copy a signing identity or activate a validator.
The local operation registry is the existing ignored location
`artifacts/multi-host-nuas/validator-D/host.json`. This is a role label only, not
an OS hostname change, node configuration, key generation or deployment entry.

## Synchronization and documents read

The original checkout was clean on main at
`41782197700a64757503eec4f7ef806e5eedab14`. GitHub main was safely fast-forwarded
by 100 commits to `46054c8e5b608538e0f58af8c15b03a84b6e6595`; no branch, reset,
force-push or automatic stash was used. This is the read/collection baseline,
not A's approved common build or deployment SHA.

Read: AGENTS.md, current multi-host handoff and read-only return template,
delivery alignment, production readiness, architecture/rebuild direction,
first-release signature/purpose identity contract, A/B installation receipt,
and C's network return and corrected SDK migration report. In particular:

- A retains shared implementation/integration ownership. This D does not
  duplicate C's App/phone work or modify shared protocol/economic code here.
- B's isolated program installation and semantic CPU verification are recorded,
  not chain activation. Old Linux SO failures are historical, not its current
  installation result. D does not reconnect to or redeploy B.
- The ordinary asset-controller and v2 concrete-account contracts supersede
  mandatory grants for ordinary payments. No asset/account or billing operation
  is performed here. The latest monthly subscription design supersedes the old
  default pay-as-you-go customer debit; overall allocation is 80/20/0, with
  implementation, exact terms and activation still separate gates.
- C's SDK migration oracle was corrected on both old/new pins. That test-contract
  correction is not evidence of a new upstream transport algorithm fix; broader
  App/device acceptance remains C's task.

## Current local return

| Field | Actual observation and boundary |
| --- | --- |
| host_label / actual role | validator-D / Windows ASUS desktop; latest direct user assignment |
| OS / architecture | Windows 11 Pro 10.0.26200, x86_64; HypervisorPresent=true, not proof that this host is a cloud VM |
| CPU / RAM | Ryzen 9 9950X, 16 cores/32 logical CPUs; about 64 GB installed, about 14.8 GiB OS-available at sampling; not exclusive capacity |
| Data disk | D free 2,708,038,328,320 bytes in the original inventory; snapshot only |
| Existing workload | `NOVOVMExecution` PID 24544 was Running; preserve the separate MEV canonical-execution/catch-up owner and its data/children. This is not proof that a NOV validator is active. Other occupied ports/process names are in JSON; no process arguments were collected |
| Physical interface | Marvell AQtion 10Gbit adapter, interface 16, current negotiated link **100 Mbps**; hardware name is not actual throughput. Intel I226-V and Wi-Fi are not the selected active physical link |
| Clock | W32Time Running/Automatic; status exit 0 reports leap 3, stratum 0, `Free-running System Clock` (not synchronized). Separate source query returned access denied 0x80070005; actual cross-host offset unknown |
| Current binaries | node/client/ctl absent in this checkout's `target/runtime-rebuild/release`; no old program was substituted, built or run |
| AOEM materialized file | `aoem/windows/core/bin/aoem_ffi.dll`, 44,705,792 bytes; SHA256 `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`, matches SDK manifest. Not loaded or qualified against the current unified-semantic candidate; cannot inherit A/B's new Linux core qualification |
| Config / DB / journal | No approved D common config, chain domain, purpose public keys or validator store identified. Only paths/status checked; no database or private-key contents read, no cleanup or creation |
| First blocker | A must accept the corrected physical host mapping and confirm common topology/ports plus the approved source/platform artifact/configuration inputs. This snapshot is not a four-host runtime qualification |

## Addresses, routes, ports and rules published for A

The [complete allowlisted JSON](evidence/validator-D-network-2026-11-02.json)
was sampled starting `2026-11-02T00:54:11.0585824Z`
(`2026-11-02T08:54:11.0585824+08:00`). The unsynchronized host clock is an
observation timestamp, not evidence of accurate cross-host UTC alignment.

JSON SHA256:
`06d19284e52ca7154012a1e0ab0c1dd872f66c429caf4c0549e5cccd9e44114a`.
It contains 78 active routes, 43 TCP listeners, 64 UDP endpoints, interface/DNS
data and three matching NOVOVM firewall rules. Counts/PIDs are instantaneous.
Unrelated firewall rule lists, process command lines and secret configuration
contents are not exported.

| Item | Actual observation | Qualification |
| --- | --- | --- |
| Physical LAN | `192.168.71.59/24`, interface 16 | Local address, not verified from A/B/C |
| C route selection | Published C LAN `192.168.71.102` selects interface 16, source `192.168.71.59`, on-link `192.168.71.0/24` | Find-NetRoute is local selection only, no probe or delivery evidence; apparent same subnet is not a connectivity PASS |
| B route selection | Published B `47.103.59.206` selects Meta/interface 44, source `198.18.0.1`, next hop `198.18.0.2` | TUN/VPN route, not physical direct evidence or a cause attribution; not modified |
| Native UDP candidate | bind `0.0.0.0:41050`, LAN candidate `192.168.71.59:41050` | No listener at sampling; not reserved/approved, no remote direction tested. Wildcard is not an advertised endpoint |
| Existing UDP allow rule | `NOVOVM Overlay Gate UDP 41020-41050`, inbound Allow/Any profile, Any source/program | Existing broad rule observed, not added or tightened. A must review exact approved sources/ports and effective policy before use; it does not prove router/cloud reachability |
| Other matching rules | `NOVOVM Cut25 UDP 41020`; raw-ceiling bench UDP39011 with its benchmark program path | Historical rules, not approval to use these ports/programs for the current validator |
| Other local networks | Meta `198.18.0.1/30`, WSL `172.21.96.1/20`, LetsTAP `26.26.26.1/29` (Tentative), loopback | Do not automatically advertise virtual, tentative or loopback addresses |
| NAT / public endpoint | WAN/public address and router forwarding unknown; NetNat and static-mapping CIM queries fail with invalid class; portproxy output retained | Failure is not an empty/absent-NAT PASS. No external IP discovery, UPnP, router operation or remote SSH/UDP/TCP probe |
| Host firewall | Both active connection profiles Public; firewall profile snapshots and relevant filters in JSON | No firewall/routing/security-group change; not a complete effective-policy or remote-reachability audit |

## Previously private historical network evidence

The original local report remains
`target/four-host-prep-20261102-validator-D-Windows.json`; this public JSON includes
its non-secret inventory fields. The older ignored
`artifacts/network-overlay-gate/real-adaptive-overlay-4node-20260703` contains
seven node JSON reports. Their exact paths/SHA256 and selected network-only
fields are now published in `legacy_adaptive_overlay_evidence` within this JSON,
without uploading full node configurations or opaque payloads.

Those records include old direct/relay/multihop/queue outcomes and old LAN
addresses. They are history, NOT a test of the current binaries, signatures,
ports, A-D hosts, NAT or reachability. No historical PASS is promoted into the
current four-host acceptance, and no ignored directory is force-added wholesale.

## Report to validator-A and requested acknowledgment

Status: `D_ROLE_RECORDED / MAIN_SYNCED / NETWORK_METADATA_PUBLISHED /
REMOTE_CONNECTIVITY_NOT_TESTED / WAITING_FOR_A`.

A can consume this GitHub return and the JSON directly. Please acknowledge:
`received_commit / evidence_sha256 / accepted_D_physical_host / old_D_role_resolution /
common_source_and_platform_artifacts / endpoint_sources / chosen_ports /
required_rule_changes / first_blocker / next_authorized_phase`.

Collect A's current network record, C's published record and B's current host/cloud
rules before selecting the topology. Clock synchronization and the 100 Mbps link
are prerequisites to assess, not silently repair during inventory. No build,
package installation, service/time/network change, key/genesis/account creation,
funding/spending or validator start occurred. Git publication is a report sent
for A to pull; it is NOT acknowledgment that A has read or accepted it.
