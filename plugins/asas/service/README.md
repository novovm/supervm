# ASAS

NOVOVM native off-chain L3 application plugin. Canonical source: `plugins/asas/service`.
Remote Agent / Local Workspace. A runs the unmodified official Claude Agent SDK;
ACODE on B executes tools in B's authorized project. No second Codex agent loop
wraps Claude. B does not need Claude Code or A's model credentials.

## Design And Status

[Architecture](docs/ARCHITECTURE.md) · [Protocol](docs/PROTOCOL.md) ·
[Security and translation](docs/SECURITY.md) · [ACODE UI](docs/CLIENT-UI.md) ·
[Development and acceptance](docs/DEVELOPMENT.md) · [Operations](docs/OPERATIONS.md).
These are development contracts, not a claim of completed cross-device delivery.
Native host and synchronization instructions: [ASAS plugin](../README.md).
Codex and Claude have separate engine identities and native sessions. The real
Codex adapter, NOVONET route and ACODE UI connection remain unfinished.

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

## Planned Native ACODE B

The ACODE client and local Worker foundation exist; native menus/settings and the
actual session route are not connected yet. The production connection will use
the existing NOVONET owner, not this HTTP development listener. All file/IDE
actions stay on B. Local GPT/SGM paths retain the existing Codex core.

## Recovery And Scope

Authenticated HTTPS with bounded, immediate long-poll event delivery. TLS provides
confidentiality, not censorship immunity or automatic NAT traversal. A needs a
reachable endpoint; a relay is a separate explicit deployment, not silently used.
NOVORUDP has NOT been integrated. Reviewed reference: NOVOVM `8f66da37065f`.
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
patches, and the existing ACODE IDE host. Arbitrary shell and administrator access
are not exposed. SGM semantic review, global task scheduling, file rewinding,
cross-device deployment and 24-hour qualification are NOT completed capabilities.
`npm test` uses explicit engine fixtures and real HTTP; it is not Claude inference.

Sources: [Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview),
[MCP](https://code.claude.com/docs/en/agent-sdk/mcp),
[authentication and branding](https://code.claude.com/docs/en/legal-and-compliance).
