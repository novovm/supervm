# ASAS UCA Login

2026-11-25. A runs NOVOVM/ASAS and official engines; ACODE is required only on B.
Installing AOEM-CODE on A alone does not install or start the ASAS provider.

## Native Path

1. B connects through the original pinned NOVONET device identity and session.
2. A reads the UCA grant through the original `novovm-native-client`, using A's
   independently pinned node configuration, not a root/result supplied by B.
3. A issues a random, 30-second, one-use challenge binding chain/genesis, UCA,
   ASAS purpose, A service, B device, grant revision and native session.
4. B checks its pinned scope and signs with its protected device key and the
   separate ASAS login context. No UCA root key or A model token enters ACODE.
5. A consumes the challenge before verification and reads fresh grant state
   again. Only then may capabilities or task operations reach the worker.
6. Every protected request checks current state. Revocation, a new revision,
   inactive identity, expiry, unavailable observations or retired sessions deny
   access. Reauthorization requires a fresh challenge.

The state source reuses the original native SDK and query correlation. It
checks status/grant/status against one published head and rejects rollback.
This is observation from an operator-approved node, not an independently
verified portable finality proof, anonymous credential or ZK login. There is no
parallel authority database. Existing project/tool approvals remain separate.

## A Configuration

Build the existing programs from NOVOVM `main`:

```sh
cargo build --release -p novovm-node --features iroh-transport --bin novovm-native-client
cargo build --release -p novovm-relay --features iroh-transport --bin novovm-rendezvous-service
```

Keep the existing ASAS worker and `network_peers` project-policy bindings. Add
this fragment to A's existing `asas.json`, using actual absolute paths/digests:

```json
{
  "uca_authority": {
    "executable": "/absolute/novovm-native-client",
    "executable_sha256": "ACTUAL_EXECUTABLE_SHA256",
    "config": "/absolute/approved-native-read-client.json",
    "config_sha256": "ACTUAL_CONFIG_SHA256"
  }
}
```

On Windows use absolute Windows paths and `.exe`. The read-client configuration
is the original native SDK profile with pinned chain/genesis/protocol, validator
and transport keys, and protected nonvalidator query-device identity. It is not
B's ASAS profile. The host starts a read-only private pipe; no public listener,
ledger or consensus owner is added.

A must observe an already-published ASAS grant. The V4 domain needs its proper
approvals; editing an old chain's version field does not upgrade/initialize it.
This login work does not create UCA identities, sign root grants or deploy nodes.

## B And ACODE

```sh
cargo build --release -p novovm-relay --features iroh-transport --bin novovm-asas-client
```

The original B client's `--client-contract` reports `ucaLogin: 1`. Its pinned
public connection profile gains `uca` with the registered `identity` string and
the actual 32-byte `genesis` array. Existing chain ID, A public-key pin and B
protected identity path remain unchanged. No private key or model token is
imported through the UI.

Linux profiles may pin `credentials_directory` to the private directory holding
their original custody `owner-key`. The native CLI selects it before AOEM or
Tokio starts; the extension never receives the credential bytes and does not
inherit another application's credential environment. Its contract reports
`credentialDirectory: 1`. The existing custody owner still checks UID, private
permissions and symlinks. Windows profiles omit this Linux-only field.

The existing authorization owner can obtain the exact public grant selector:

```sh
novovm-asas-client --profile /absolute/B.json --profile-sha256 ACTUAL_SHA256 --uca-scope
```

This validates the protected key and derives its public reference. It opens no
network, grants no permission and does not log in. Service/device references use
the original UCA ML-DSA binding function, not transport-node fingerprints.

ACODE's account/shared-computer entry shows the native client's authenticated
UCA and exact decimal revision. Configuration import and transport handshake
alone are not UCA login. Claude and Codex use this same admission layer.

## Boundaries

- Missing A `uca_authority` preserves explicit-node mode, never labelled UCA.
  A B profile selecting UCA cannot downgrade; older B clients must be rebuilt.
- Admission lasts at most 15 minutes and is not a transferable bearer token.
  A restart or native-session change requires a new login.
- Revocation stops subsequent requests, not already-started effects; it cannot
  undo an earlier authorized edit or command.
- Automatic service discovery/common relay selection, real two-machine login
  and remote official-engine programming acceptance remain separate work.
- No NOV charging, credential migration, model training or SGM changes.

## Focused Verification

ARM64 source checks cover the original node library/query client and ASAS host/B
client. Native-session admission tests cover replay, scope tampering, expiry,
revocation and revision changes; the same suite passed with actual AOEM ML-DSA
signatures. Grant observations in these tests are fixtures, not live chain data.
ACODE type checks, public-metadata/config tests and 280/668-pixel login UI checks
passed. No remote A deployment or dual-host engine acceptance is claimed.

## 2026-11-26 validator-C / validator-F handoff

- `validator-C` is the ASAS provider. Its Windows host built the pinned-query client, Rendezvous and Node Worker. Official Claude Code and Codex reported authenticated local accounts and returned model catalogs. The existing node is running. ASAS startup, published-grant login and remote coding are pending.
- The current Windows session cannot read the running `validator-C` service configuration or create the Codex login file symlink. The existing service operator should confirm the original NOVONET owner to use, the approved native read-only profile, and the public service key and endpoint. The provider-account operator should complete the Codex link with the existing credential owner and verify access under the runtime account.
- `validator-F` runs ACODE and owns the project tools. Its session can return the authenticated device node ID, public ML-DSA-65 device key and canonical project digest now. After receiving the provider pin and approved chain/genesis, it can return `--uca-scope` for the exact grant. Send public values through the existing authorized handoff channel. Keep private profiles, identities and engine credentials on their owning host.
- The UCA grant owner publishes the matching ASAS grant. The provider verifies the published revision using its pinned read-only SDK profile. Acceptance requires a native UCA login from `validator-F` and a separate task through each official engine, with edit/build/test receipts produced on `validator-F`.

## Existing-chain registration proof

`novovm_protocol::uca_registration` defines the bounded canonical `NUIR` proof. `novovm_exec` verifies it with the original qualified ML-DSA-65 owner, using a distinct registration context and an explicit original chain ID, genesis and half-open height range. The initial registration name is `uca:` plus the lowercase hex of the existing ML-DSA-65 primary-key reference. `validator-F` can compare that name to its prepared selector on its own machine without exporting root key material. Existing assigned names retain their original bindings.

This proof checks key possession. Native admission, the no-overwrite update at the published parent, a coordinated activation boundary across current validators, history replay, and the signed application grant remain required. The registration proof does not change any live policy or admit network input by itself.

The native business layer prepares the bounded `/identity/` changes after checking the admitted proof against the block and the captured parent. It refuses an existing identity or any orphan page in the identity's declared range. The new proof tag is appended to the existing account codec, and the legacy `ua_createUca` surface cannot use it. The current finalized-state writer and ASAS grant check still require activation wiring before this effect can be used on the chain.

Existing validator operators can acknowledge the upgrade work against the same chain ID, genesis and current validator-set commitment. After an executable upgrade is built, use the existing quorum rules and agree on one exact parent checkpoint and activation boundary. No activation height or validator signatures have been collected. `validator-F` is the ASAS consumer and does not acquire a validator vote by registering a UCA.

The `round_bft::wire::activation` verifier uses the existing validator set and weighted quorum. Its separately framed statement binds the first upgraded block's height, original chain/genesis/protocol, parent block and decision, parent state and receipt roots, and both business profiles. Consent is not a block vote or a recovered canonical parent proof. There is no operator signing entry or live activation from this verifier alone.

Registration candidates use a separate V5 profile and `NVCAND06` archive. Candidate reconstruction must verify the original signed command and recompute the identity effect and commitments from the pinned parent. The archive verifier checks the exact command receipt and access declarations and supplies no asset projection. The original live ingress, quorum activation owner and cold journal still need V5 wiring before a validator can vote on these candidates.

`validator-F` confirmed that its existing protected root derives the prepared UCA name. Keep that original selector. For coordination, current validator operators should report the same finalized checkpoint and deployed binary identity once the full upgrade candidate is available. Start signing only after the exact transition is reviewed on each validator. Keep the current services on their existing policy during preparation.

### F preparation and C follow-up

F has generated independent protected ML-DSA-65 transport and identity-purpose
keys through the original `novovmctl native-identity` owner and exported the
native client's exact `--uca-scope`. Public material and C's next actions live
in AOEM-CODE `docs/asas/validator-F-public.json` and
`docs/asas/validator-C-next.md`; private keys and wrapping credentials stay on F.

This does not register a UCA. `business/uca_identity.rs` currently requires a
`GenesisInitializationV1` binding for application authorization; local
`UnifiedAccount` creation is not an opening through the published native tree.
The present F identity is not published. C must first query its pinned chain's
actual identity/grant state and coordinate the native registration gap with the
chain owner. Do not patch the live database, alter genesis, substitute another
UCA, issue a synthetic revision or bypass admission to make ASAS appear ready.

The C-advertised mailbox candidate timed out through F's actual ACODE connection
entry; neither native UCA login nor remote programming passed. Native identity
creation reused the existing CLI whose identity command source is unchanged;
a current CLI rebuild failed on a truncated local RocksDB archive. That failed
rebuild is not reported as passed. The ASAS B client build is independent of it.
