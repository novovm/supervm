# ASAS

Current priority (2026-11-15): usable shared programming first; further token/NOV
work is deferred. Real Codex completed a small Rust library/CLI, saw failing tests,
fixed source, compiled and passed five fixed tests plus an independent fresh-input
check on B's tools in same-device isolation. Daily VS Code and real dual-host
Claude/Codex acceptance remain unfinished. See [actual coding evidence](docs/CODEX.md).

Product direction: free, authorized shared AI programming services on NOVOVM,
not primarily a failover tool. Record network bytes and model-token usage without
charging users or moving NOV. Any optional NOV reference conversion needs an
explicit rule; statistics are not wallet balances or rewards.
[Service-network baseline](docs/SERVICE-NETWORK.md) records the latest direction,
authorization boundary and remaining multi-A/network-metering work.

NOVOVM native off-chain L3 application plugin. Canonical source: `plugins/asas/service`.
Remote Agent / Local Workspace. A runs official Codex or the Claude Agent SDK,
without an ACODE runtime dependency;
ACODE on B executes tools in B's authorized project. No second Codex agent loop
wraps Claude. B does not need Claude Code or A's model credentials.

## Design And Status

[Architecture](docs/ARCHITECTURE.md) · [Protocol](docs/PROTOCOL.md) ·
[Security and translation](docs/SECURITY.md) · [ACODE UI](docs/CLIENT-UI.md) ·
[Development and acceptance](docs/DEVELOPMENT.md) · [Operations](docs/OPERATIONS.md).
[Platform AOEM Tool Contract and accepted ADRs](docs/AOEM-TOOL-CONTRACT.md) define
the shared execution standard and prioritize real official Codex/Claude sharing.
These are development contracts, not a claim of completed cross-device delivery.
Native host and synchronization instructions: [ASAS plugin](../README.md).
Codex and Claude have separate engine identities and native sessions. The real
Codex adapter and same-device native pipe have passed actual inference and ACODE
file-tool checks in same-device isolation: [Codex setup](docs/CODEX.md). The spawning
probe lives in B's test support only. A accepts opaque B project IDs, never resolves
B workspace paths. The native UDP application route now exists on the original
node listener: [NOVONET boundary](docs/NOVONET.md). The B-only native process interface
and ACODE shared-node settings/chat/tool integration are implemented and component-tested.
Real node deployment, signed service advertisement, qualified fallback and cross-device acceptance remain unfinished.
SDK 0.6 adds authenticated B-node task/session binding and shared project skill
discovery/read tools. B explicitly approves a content-addressed skill bundle;
same-name skills on other B nodes/projects are distinct. No global skills are
scanned or installed on A. See the platform contract for scope, limits and legacy
recovery requirements. Native A hosts must also update to pass the authenticated node ID.

SDK 0.7 adds persistent Claude usage, per-authenticated-B token allocations,
live owner-only limit/reset commands and B-only quota views. Official Claude
plan windows remain separate and unknown when unavailable. In-flight requests
can exceed a local cap; interrupted final usage is not assumed zero.
See [quota operation and accounting](docs/QUOTAS.md). No billing or NOV transfer.

SDK 0.8 adds versioned, model-scoped native reasoning/permission capabilities and
optional per-turn reasoning effort / Claude Auto requests. Claude capability
discovery initializes its official CLI without sending a user prompt; Auto is
advertised only after a successful native mode-control request. Codex reads its
pinned native catalog. Unknown metadata never becomes a guessed capability.
Auto has no blanket MCP allow rule or allow-on-classifier-refusal fallback. B's
policy remains authoritative. Settings changes keep the original session and do
not replay tasks; actual remote Claude inference remains unqualified.

UCA challenge admission is wired into the existing native A route and B adapter:
[setup and boundaries](docs/UCA-LOGIN.md). A reads fresh published grants through
the original node SDK; B signs with its protected device key. Service, device,
revision and native session are bound; protected requests recheck revocation.
Explicit-node compatibility is not UCA authentication. Common relay selection
and real dual-host acceptance remain separate work; no accept-all identity
bridge or B-supplied authorization result is used.

## Run A Development Contract

Node 22 or newer. `npm ci --ignore-scripts`, then `npm run build`.
Run `node dist/main.js pair /canonical/B/project B` ON B (or calculate SHA-256 of
B's canonical workspace path). It prints a peer grant and a one-time random token.
Keep the token in B's ACODE SecretStorage; only its hash belongs in A's config.
Create a private `config.json` on A:

```json
{
  "directory": "/absolute/private/asas-data",
  "host": "127.0.0.1",
  "port": 7843,
  "models": ["default"],
  "peers": [{"id":"B","tokenSha256":"REPLACE_WITH_64_HEX_HASH","projects":["REPLACE_WITH_B_PROJECT_HASH"]}]
}
```

Configure A's own authorized Anthropic API/cloud credentials using the official
SDK setup. No credentials are copied from another application. `executable` may
explicitly select A's unmodified official CLI. Subscriptions are not assumed to
authorize pooled multi-user service. Model calls incur usage: current engine
limits are 100 turns and USD 5 per submitted task, not a global spending cap.

`npm start -- /absolute/config.json` starts the service, not inference. For remote
access, expose it through a TLS 1.3 listener (`tlsFiles.cert`, `tlsFiles.key`) or a
trusted HTTPS reverse proxy. Never publish the loopback HTTP listener directly.
Use one independently revocable peer token per device; project grants are exact.

## Native ACODE B

The native B client, settings and direct session route are implemented with the
existing NOVONET owner, not this HTTP development listener. Both official engines
derive declarations from one contract. All file/IDE/command actions execute on B.
Local GPT/SGM paths retain the existing Codex core. Real cross-device acceptance is pending.

## Recovery And Scope

Authenticated HTTPS with bounded, immediate long-poll event delivery. TLS provides
confidentiality, not censorship immunity or automatic NAT traversal. A needs a
reachable endpoint; a relay is a separate explicit deployment, not silently used.
Native NovoRUDP application dispatch is implemented as described above, but not
cross-device-qualified. ACODE's daily UI is connected in development. Historical transport review: NOVOVM `8f66da37065f`.
Its current authoritative network policy is native NovoRUDP first, with qualified
Iroh fallback under the existing NOVONET owner. The former WSS/Relay main-chain
carrier is retired; NOVOCHAT also removed its old WSS client. Remaining generic
relay source files do NOT establish an approved ASAS transport or active product
route. The root Cargo workspace explicitly excludes `runtime`; application
integration must inspect the current `crates/novovm-network` / NOVONET owners,
not infer current behavior from `runtime/novovm-network` source. See
`AGENTS.md:78`, `AGENTS.md:137`, and
`docs/NOVONET_TRANSPORT_POLICY_V1.md` in the synchronized NOVOVM checkout.
The HTTPS implementation here is an isolated application-protocol baseline, not
NOVONET integration, not a replacement network and not accepted cross-device
deployment. Public network selection remains unqualified. No blockchain,
historical transport or identity store is copied into ASAS.

Tasks, sessions and events are private durable records. Admission IDs are stable;
receiving an ACK is not completion. One writer owns a peer/project/model session.
After interruption with an unresolved tool, or service restart during execution,
the state is NEEDS_RECOVERY. There is no automatic rerun. The operator must inspect
A's task and B's tool journal before creating a new session. Do not delete these
records to pretend an uncertain write never happened.

Initial scope: bounded workspace listing, text reads/search, hash-checked exact
patches, the existing ACODE IDE host and approved bounded argv commands for B
terminal/build/test/Git. Commands have ordinary B user authority, not a filesystem
sandbox; no implicit shell or administrator access. Side effects use a shared project
lock; operation journals are exclusively claimed and uncertain effects never replay.
SGM semantic review, global task scheduling, file rewinding,
cross-device deployment and 24-hour qualification are NOT completed capabilities.
`npm test` uses explicit engine fixtures and real HTTP; it is not Claude inference.

Sources: [Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview),
[MCP](https://code.claude.com/docs/en/agent-sdk/mcp),
[authentication and branding](https://code.claude.com/docs/en/legal-and-compliance).
