# Windows relay installation and persistent firewall

Date: 2026-10-11. Input: `adbf7f07`; claim: `7e22f00`.
The user requested persistent node permission, replacing the temporary firewall
experiment. Installation now installs the existing relay and its rule together.

## Entry and ownership

- [Installer](../../scripts/windows/install-novovm-relay.ps1)
- [OS policy module](../../scripts/windows/NovovmRelayFirewall.psm1)
- [Policy regression](../../scripts/tests/windows-relay-install.tests.ps1)

`Install` requires administrator consent and an independently verified binary
SHA-256. It installs under the OS Program Files directory at
`NOVOVM\Relay\supervm-relay.exe`, records ownership, then creates or repairs one
rule. Repeat installation uses the same path and rule. Unmanaged installations
and conflicting rule ownership are preserved and rejected.

| Property | Managed value |
| --- | --- |
| Rule name | `NOVOVM-Relay-WSS-TCP-443-v1` |
| Owner group | `NOVOVM Relay managed installer v1` |
| Store | `PersistentStore`, checked against `ActiveStore` |
| Direction / action | Inbound / Allow |
| Program | Exact installed `supervm-relay.exe` path |
| Protocol / local port | TCP / 443 |
| Profiles | Domain, Private, Public (`Any`) |
| Local / remote addresses | Any |
| Edge traversal | Block; no automatic NAT mapping |

The rule survives node exit and is stored persistently for subsequent boots.
No firewall profile is disabled or reclassified. Outbound policy is unchanged.
Unreadable policy is an error, not an absent rule. Readback checks program,
port and address filters, but does not override enterprise policy or unrelated
explicit block rules.

`Uninstall` removes only the owned rule and executable/manifest, and checks both
policy stores. It does not recursively delete directories, remove unknown
files, or remove external identities, certificates, configuration or data.
The runtime is not killed to force an update; stop it before upgrading if
Windows reports a file lock.

## Use

From the repository root, in an administrator PowerShell for Install/Uninstall:

```powershell
# Supply the independently checked release digest.
.\scripts\windows\install-novovm-relay.ps1 -Action Install -SourceBinary <verified-executable> -ExpectedSha256 <verified-sha256>
.\scripts\windows\install-novovm-relay.ps1 -Action Status
# Only when intentionally removing this installation:
.\scripts\windows\install-novovm-relay.ps1 -Action Uninstall
```

`Status` does not require elevation. Optional `-ReportPath` writes JSON to an
absolute path in an existing directory. Failure returns `ok:false` and exit 1,
including missing administrator consent or unmatched effective policy.

This is the Windows relay package installation entry. Future Windows packaging
should invoke it rather than build another firewall implementation. Full-node
GUI/MSI packaging and an always-running Windows service are not delivered here.
Installation does not invent node identity or start a runtime. With a separately
provisioned configuration, run as a normal user:

```powershell
$relay = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'NOVOVM\Relay\supervm-relay.exe'
& $relay --runtime duplex-v2 <node-config.json>
```

The actual call chain is `crates/novovm-relay/src/main.rs ->
product_relay_launch::run_product_relay_cli_v1 ->
duplex::product_relay_daemon::run_product_relay_daemon_v1`.
No legacy fallback is introduced. The rule applies to the installed executable,
not arbitrary development binaries. Discovery, public reachability and TLS
identity remain separate transport responsibilities.

## Actual verification

On this Windows host with user-approved UAC:

1. Install and repeat Install passed. Exactly one owned rule exists; persistent
   and active filters match. Independent final Status passed.
2. Installed the previously built and process-tested SUPERVM relay, 9,604,096
   bytes, SHA-256
   `ac24be61a5a71b8ebf0a991995c7734e6698e0bfaef6a906e30d7e8b29949cc8`.
   This slice did not rebuild or change the executable.
3. Ran that installed program as a normal user, explicitly `duplex-v2`, on the
   laptop's actual WLAN interface TCP 443 with a 120-second engineering fixture.
   The runtime confirmed its listener and later graceful shutdown, with zero
   active connections and sessions. The persistent rule remains installed.
4. PKL110 Android on Wi-Fi passed TLS and pinned node identity verification.
   The entire ADB probe command took 0.316 seconds (not pure RTT or a benchmark).
   Host positive control also passed. ADB forward/reverse tunnels were absent.
   Phone default network stayed Wi-Fi; PCLM50 stayed cellular and was not tested
   against a private LAN address. Only a probe and public CA were copied onto
   PKL110, verified by hash, and removed afterwards.
5. Final policy fixture regression: 19 assertions passed, covering broad port
   rejection, unreadable policy, foreign ownership, inactive policy, repeated
   installation and idempotent removal. Three PowerShell files parse. Fixtures
   do not mutate the OS. Real uninstall was intentionally not run because the
   user wants the permission retained. No system reboot test was performed.

OS evidence is ignored under `artifacts/windows-relay-install/`. Phone/node
evidence is in NOVOCHAT's ignored `artifacts/persistent-node-latest.json` and its
run directory. Earlier probes timed out before authentication; this establishes
working local ingress with the installed program and rule, without proving
every earlier failure had the same cause.

The probe sent no chat messages (`peer_delivery:false`). Explicit engineering
CA and temporary node identity are not production UCA authority or normal app
WebPKI acceptance. No router changes, cross-cellular chat, automatic NAT success,
anonymity or financial operations are claimed.
