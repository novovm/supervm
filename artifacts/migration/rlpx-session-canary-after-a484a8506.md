# RLPx Session Canary After a484a8506

Status: diagnosis-only canary report.

Scope:
- This report investigates where one public RLPx probing path stops when auth is sent but no ack is observed.
- It does not change geth-facing RPC compatibility, BAL guard behavior, or NOVOVM plugin architecture.
- The gateway uses isolated state paths under `D:\WEB3_AI\SUPERVM\artifacts\migration\state\rlpx-layered-canary-1779268005038` and does not reuse `artifacts/gateway/unified-account-router.rocksdb`.

Layered Results:

### local controlled geth peer

- status: `skipped`
- reason: `LocalGethEnode was not supplied; this diagnostic does not spawn a geth peer`

### public discovery-only

- status: `completed`
- discovery_ping_sent_count: `0`
- discovery_pong_seen_count: `0`
- dns_discovery_query_sent_count: `1`
- dns_discovery_enode_seen_count: `1`
- discovered_peer_count: `1`
- candidate_session_peer_count: `1`
- note: `DNS ENR discovery is exercised here; UDP discv4 ping/pong is not performed by this diagnostic and is not treated as session acceptance.`

- remote_node_id=`6be8358ecdbfa099838a0da7ba3687a7e3d07ca30a3c190e9e6ab0580cb9da533c46ca823c8597420ba8334d6dcce24f265b36b589679714520ebd104194b3ad` endpoint=`enode://6be8358ecdbfa099838a0da7ba3687a7e3d07ca30a3c190e9e6ab0580cb9da533c46ca823c8597420ba8334d6dcce24f265b36b589679714520ebd104194b3ad@64.34.94.23:30303` remote_enr=`enr:-KO4QA_bMSt78e8KPHhXt8hmC6tOmoLjqnGewDyLnYPnaMiFRaUvkoeV_myTbpwPM1qRUTa2JqSq4HxjwLTi7dyfMqWGAZtFAabZg2V0aMfGhAfJRi6AgmlkgnY0gmlwhEAiXheJc2VjcDI1NmsxoQNr6DWOzb-gmYOKDae6Noen49B8owo8GQ6earBYDLnaU4RzbmFwwIN0Y3CCdl-DdWRwgnZf`

### public discovered-peer session

- status: `completed`
- tcp: attempts=`2`, success=`1`, fail=`1`
- auth: sent=`1`, ack_seen=`0`, timeout=`0`, disconnect_before_ack=`1`
- p2p/eth: hello_sent=`0`, hello_seen=`0`, status_sent=`0`, status_seen=`0`, ready=`0`
- selected_eth_capability: `none`
- disconnect_reason_code: `4`

Compact traces:
- peer=`6be8358ecdbfa099838a0da7ba3687a7e3d07ca30a3c190e9e6ab0580cb9da533c46ca823c8597420ba8334d6dcce24f265b36b589679714520ebd104194b3ad` endpoint=`64.34.94.23:30303` stage=`disconnected` best=`disconnected` reason=`rlpx_remote_disconnected_before_hello:reason_code=4 reason=too_many_peers` cap=`none`
- peer=`6be8358ecdbfa099838a0da7ba3687a7e3d07ca30a3c190e9e6ab0580cb9da533c46ca823c8597420ba8334d6dcce24f265b36b589679714520ebd104194b3ad` endpoint=`64.34.94.23:30304` stage=`disconnected` best=`disconnected` reason=`connect_failed(64.34.94.23:30304):connection timed out` cap=`none`

Current Diagnosis:
- Local controlled geth session was not exercised because no local enode was supplied.
- Public DNS ENR discovery produced candidate session peers; bootnode/DNS discovery is not treated as eth session readiness.
- Public discovered-peer session stopped below auth ack in this run.

Interpretation:
- If the local controlled peer passes but the public session stops before ack, the likely area is public peer selection, endpoint reachability, network egress, or remote policy.
- If both local and public sessions stop before ack, the next independent patch should inspect RLPx auth/session details.
- A run that does not observe ack also does not proceed far enough to observe Hello, Status, or eth capability negotiation in that run.
- This does not mean the NOVOVM EVM plugin lacks Hello/Status handling.

Not Claimed:
- no protocol fix
- no full eth/71 or BAL implementation
- no old UnifiedAccountRouter state migration
- no new NOVOVM plugin architecture
