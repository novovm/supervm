# Remote Controlled Geth RLPx Canary After a484a8506

Status: public RLPx readiness/status failure classification report.

Scope:
- This report records public discovered-peer RLPx readiness progress and the current failure class by using peer candidate diversity, endpoint filtering, cooldown, and failure-stage accounting.
- It does not change geth-facing RPC compatibility, BAL guard behavior, or NOVOVM plugin architecture.
- Bootnode and DNS discovery targets are discovery inputs only; readiness is assessed only against discovered session peers.
- Gateway executable: `D:\cargo-target-supervm\debug\novovm-evm-gateway.exe`.
- The gateway uses isolated state paths under `D:\WEB3_AI\SUPERVM\artifacts\migration\state\rlpx-layered-canary-1779365071472` and does not reuse `artifacts/gateway/unified-account-router.rocksdb`.

Prior Evidence:
- Local controlled geth evidence from the previous follow-up showed TCP, RLPx auth ack, Hello, Status, negotiated eth/69, and ready_count=1.
- Earlier public short-window samples stopped below auth ack and observed too_many_peers / TCP timeout outcomes.
- RemoteControlledGethEnode is a controlled public-network comparison point; it is reported separately from random public discovered-peer readiness.

Public Peer Selection Changes:
- DNS ENR discovery can collect a larger candidate pool before session attempts.
- Public session candidates are filtered for usable public endpoints.
- Session attempts are spread across candidates and rounds instead of treating the first discovered peer as the whole public result.
- Peers returning too_many_peers are cooled down for later rounds; TCP timeout endpoints are penalized.
- eth/68-only Hello samples are classified separately and cooled down so eth/69 or eth/70 peers remain the readiness target.
- Candidate port diversity is controlled by PublicPluginPorts.

Layered Results:

### local controlled geth peer

- status: `skipped`
- reason: `LocalGethEnode was not supplied; this diagnostic does not spawn a geth peer`

### remote controlled geth peer

- status: `skipped`
- reason: `RemoteControlledGethEnode was not supplied; this diagnostic does not exercise a controlled geth peer over a public network path`

### public discovery-only

- status: `completed`
- discovery_ping_sent_count: `0`
- discovery_pong_seen_count: `0`
- dns_discovery_query_sent_count: `1`
- dns_discovery_enode_seen_count: `8`
- discovered_peer_count: `8`
- candidate_session_peer_count: `8`
- note: `DNS ENR discovery is exercised here; UDP discv4 ping/pong is not performed by this diagnostic and is not treated as session acceptance.`

- remote_node_id=`6be8358ecdbfa099838a0da7ba3687a7e3d07ca30a3c190e9e6ab0580cb9da533c46ca823c8597420ba8334d6dcce24f265b36b589679714520ebd104194b3ad` endpoint=`enode://6be8358ecdbfa099838a0da7ba3687a7e3d07ca30a3c190e9e6ab0580cb9da533c46ca823c8597420ba8334d6dcce24f265b36b589679714520ebd104194b3ad@64.34.94.23:30303` remote_enr=`enr:-KO4QA_bMSt78e8KPHhXt8hmC6tOmoLjqnGewDyLnYPnaMiFRaUvkoeV_myTbpwPM1qRUTa2JqSq4HxjwLTi7dyfMqWGAZtFAabZg2V0aMfGhAfJRi6AgmlkgnY0gmlwhEAiXheJc2VjcDI1NmsxoQNr6DWOzb-gmYOKDae6Noen49B8owo8GQ6earBYDLnaU4RzbmFwwIN0Y3CCdl-DdWRwgnZf`
- remote_node_id=`d14dcd91c765fc7953532dd3cec8c42ccb2978f583f96a9f5b79bbf7a3a10f3e30592c1839bf37bb5e03e02f3bf8770c37d46f01e351b9f7d2adaba2e0f2c760` endpoint=`enode://d14dcd91c765fc7953532dd3cec8c42ccb2978f583f96a9f5b79bbf7a3a10f3e30592c1839bf37bb5e03e02f3bf8770c37d46f01e351b9f7d2adaba2e0f2c760@3.1.24.239:30303` remote_enr=`enr:-KO4QD21Kg_Hx-2wgiNJxq3m3VbEcBk5LBRLAWxjSYpLUe0nUQGIl12f4fTJ1dbCILONSVY0dG_bHLdv1WB-gVOww5KGAZtECreDg2V0aMfGhAfJRi6AgmlkgnY0gmlwhAMBGO-Jc2VjcDI1NmsxoQLRTc2Rx2X8eVNTLdPOyMQsyyl49YP5ap9bebv3o6EPPoRzbmFwwIN0Y3CCdl-DdWRwgnZf`
- remote_node_id=`7d1e976f507518cd895e5721f4c58a5b8ee446eb04aab7b386ce5c84cc7116d10b32c1e749d84834ffd17bd5f9928ecbe56b1dd206172d24e8ba479ba1590cd6` endpoint=`enode://7d1e976f507518cd895e5721f4c58a5b8ee446eb04aab7b386ce5c84cc7116d10b32c1e749d84834ffd17bd5f9928ecbe56b1dd206172d24e8ba479ba1590cd6@13.222.122.244:30303` remote_enr=`enr:-KO4QIAqcCOaId9UQ3iMW0UD0jKw_nmPxBjCgrsb_jXn-1xmZo3T8qHvFj5mzlsHBT_uWlQuBPm1r-ac3nnmLGNskBuGAZv_S-Jhg2V0aMfGhAfJRi6AgmlkgnY0gmlwhA3eevSJc2VjcDI1NmsxoQJ9HpdvUHUYzYleVyH0xYpbjuRG6wSqt7OGzlyEzHEW0YRzbmFwwIN0Y3CCdl-DdWRwgnZf`
- remote_node_id=`b7148466c8558f57da7a16259edcaece6832400c0baaba01b4e20e60c426922791899525f217a6ffb301d1c2b2a2695963b78c5e765f85e84084ee8d2f86db7c` endpoint=`enode://b7148466c8558f57da7a16259edcaece6832400c0baaba01b4e20e60c426922791899525f217a6ffb301d1c2b2a2695963b78c5e765f85e84084ee8d2f86db7c@95.216.12.50:30303` remote_enr=`enr:-Je4QH5fmSfiVUBl9mf3oEoHzQpgRxDp1V2KxPVtZOUj8eiHebJGSxGUFi7foWOluW3UqHWZm49bKtVZ84B-1-hl4o1cg2V0aMfGhAfJRi6AgmlkgnY0gmlwhF_YDDKJc2VjcDI1NmsxoQK3FIRmyFWPV9p6FiWe3K7OaDJADAuqugG04g5gxCaSJ4N0Y3CCdl-DdWRwgnZf`
- remote_node_id=`f68ec57cfca4d606d7f03794897d7d38baf9775953bf7e4f2648d648c3c006b9f1470297cc4c3d9bcba804650a349ae743c83c45928e6a3ea50456bc46f55e52` endpoint=`enode://f68ec57cfca4d606d7f03794897d7d38baf9775953bf7e4f2648d648c3c006b9f1470297cc4c3d9bcba804650a349ae743c83c45928e6a3ea50456bc46f55e52@91.156.63.40:30303` remote_enr=`enr:-KO4QAHmA3udRaVDE2WRMkw5JC74FgI-5iSfprX6oWixTfWOJ3LWKOqUdcwpXh1i_tGMHOTzxpTRJerShTwn6-smVy2GAZxsiK3Pg2V0aMfGhAfJRi6AgmlkgnY0gmlwhFucPyiJc2VjcDI1NmsxoQL2jsV8_KTWBtfwN5SJfX04uvl3WVO_fk8mSNZIw8AGuYRzbmFwwIN0Y3CCdl-DdWRwgnZf`
- remote_node_id=`eeed69e1665475cf0793d7af596ff3f0a23847a64af921850c3aea274e6c0962c53e7d90bd7c01ce4447bc6a54b0f41b5d8894149478d083b076b108fef119f5` endpoint=`enode://eeed69e1665475cf0793d7af596ff3f0a23847a64af921850c3aea274e6c0962c53e7d90bd7c01ce4447bc6a54b0f41b5d8894149478d083b076b108fef119f5@76.224.20.214:30403` remote_enr=`enr:-KO4QLzUAsWXHgRU37e9UZjstwibO39sNHotO9jlqV2R3Te4aWmlzuMEQ2PywrLR31aAjv3WHjdGuGFykCpUowu-LYuGAZMPnKH8g2V0aMfGhAfJRi6AgmlkgnY0gmlwhEzgFNaJc2VjcDI1NmsxoQPu7WnhZlR1zweT169Zb_PwojhHpkr5IYUMOuonTmwJYoRzbmFwwIN0Y3CCdsODdWRwgnbD`
- remote_node_id=`3af4e30bc41d555942f6816fd752b29e6d245f739de4d7df3c25e0787b110c33fadefe0bc785cc144cb414ec09968a6eed334b9f1dbe43d0d3cce9fdad3a74cc` endpoint=`enode://3af4e30bc41d555942f6816fd752b29e6d245f739de4d7df3c25e0787b110c33fadefe0bc785cc144cb414ec09968a6eed334b9f1dbe43d0d3cce9fdad3a74cc@54.179.0.167:30303` remote_enr=`enr:-KO4QFedeHe7NwUuGFR2-4gNJpAi-tTlQgAC2OftvUiaqpyIPdScuFpJUFLjSh4Ben-q5Fol1Fyex__R7wUDshOrTQuGAZfo_3PRg2V0aMfGhAfJRi6AgmlkgnY0gmlwhDazAKeJc2VjcDI1NmsxoQI69OMLxB1VWUL2gW_XUrKebSRfc53k1988JeB4exEMM4RzbmFwwIN0Y3CCdl-DdWRwgnZf`
- remote_node_id=`04ed80e63eed5f00606785af5e44fd13b100b26c81619b5a3cdcef0e8c3766ef13344fcebf32b4ac017503b0bccdd7237378d8d861718f84ba5a85fbff0a75c1` endpoint=`enode://04ed80e63eed5f00606785af5e44fd13b100b26c81619b5a3cdcef0e8c3766ef13344fcebf32b4ac017503b0bccdd7237378d8d861718f84ba5a85fbff0a75c1@185.209.178.223:30303` remote_enr=`enr:-KO4QKr_3sa2UNOcJaQYHT8LvpE2i9DmBZIf3Rt-NIchR3xKazz1QrYBWvkCRfm5YUOr4GWjsu1QfqAqcvfY9ZlkLq6GAZtLuQXLg2V0aMfGhAfJRi6AgmlkgnY0gmlwhLnRst-Jc2VjcDI1NmsxoQME7YDmPu1fAGBnha9eRP0TsQCybIFhm1o83O8OjDdm74RzbmFwwIN0Y3CCdl-DdWRwgnZf`

### public discovered-peer session

- status: `completed`
- reason: `public discovered-peer session did not reach ready after 2 round(s)`
- candidates: discovered=`8`, after_filter=`17`, selected_attempts=`4`, rounds=`2`
- tcp: attempts=`8`, success=`4`, fail=`4`, timeout=`4`
- auth: sent=`4`, ack_seen=`4`, timeout=`0`, disconnect_before_ack=`0`
- p2p/eth: hello_sent=`4`, hello_seen=`1`, status_sent=`1`, status_seen=`0`, ready=`0`
- selected_eth_capability: `eth/68`
- disconnect_reason_too_many_peers_count: `4`
- disconnect_before_hello_count: `3`
- disconnect_before_status_count: `0`
- disconnect_after_status_sent_count: `1`
- disconnect_after_hello_before_local_status_count: `0`
- disconnect_after_local_status_before_remote_status_count: `1`
- capability_mismatch_count: `0`
- eth68_only_peer_count: `1`
- eth69_70_peer_count: `0`
- status_payload_mismatch_count: `0`
- endpoint_timeout_count: `4`
- peer_cooldown_count: `4`
- disconnect_reason_code: `4`

Compact traces:
- peer=`6be8358ecdbfa099838a0da7ba3687a7e3d07ca30a3c190e9e6ab0580cb9da533c46ca823c8597420ba8334d6dcce24f265b36b589679714520ebd104194b3ad` endpoint=`64.34.94.23:30303` stage=`disconnected` best=`hello_sent` class=`too_many_peers_before_hello` phase=`before_hello` reason=`rlpx_remote_disconnected_before_hello:reason_code=4 reason=too_many_peers` cap=`none` client=`` eth_caps=`` snap_caps=`` hello_ms=`` status_sent_ms=`` status_seen_ms=`` disconnect_ms=`0x5` local_status=`` remote_status=``
- peer=`6be8358ecdbfa099838a0da7ba3687a7e3d07ca30a3c190e9e6ab0580cb9da533c46ca823c8597420ba8334d6dcce24f265b36b589679714520ebd104194b3ad` endpoint=`64.34.94.23:30304` stage=`disconnected` best=`disconnected` class=`endpoint_timeout` phase=`` reason=`connect_failed(64.34.94.23:30304):connection timed out` cap=`none` client=`` eth_caps=`` snap_caps=`` hello_ms=`` status_sent_ms=`` status_seen_ms=`` disconnect_ms=`` local_status=`` remote_status=``
- peer=`d14dcd91c765fc7953532dd3cec8c42ccb2978f583f96a9f5b79bbf7a3a10f3e30592c1839bf37bb5e03e02f3bf8770c37d46f01e351b9f7d2adaba2e0f2c760` endpoint=`3.1.24.239:30303` stage=`disconnected` best=`hello_sent` class=`too_many_peers_before_hello` phase=`before_hello` reason=`rlpx_remote_disconnected_before_hello:reason_code=4 reason=too_many_peers` cap=`none` client=`` eth_caps=`` snap_caps=`` hello_ms=`` status_sent_ms=`` status_seen_ms=`` disconnect_ms=`0x7` local_status=`` remote_status=``
- peer=`d14dcd91c765fc7953532dd3cec8c42ccb2978f583f96a9f5b79bbf7a3a10f3e30592c1839bf37bb5e03e02f3bf8770c37d46f01e351b9f7d2adaba2e0f2c760` endpoint=`3.1.24.239:30304` stage=`disconnected` best=`disconnected` class=`endpoint_timeout` phase=`` reason=`connect_failed(3.1.24.239:30304):connection timed out` cap=`none` client=`` eth_caps=`` snap_caps=`` hello_ms=`` status_sent_ms=`` status_seen_ms=`` disconnect_ms=`` local_status=`` remote_status=``
- peer=`7d1e976f507518cd895e5721f4c58a5b8ee446eb04aab7b386ce5c84cc7116d10b32c1e749d84834ffd17bd5f9928ecbe56b1dd206172d24e8ba479ba1590cd6` endpoint=`13.222.122.244:30303` stage=`disconnected` best=`hello_sent` class=`too_many_peers_before_hello` phase=`before_hello` reason=`rlpx_remote_disconnected_before_hello:reason_code=4 reason=too_many_peers` cap=`none` client=`` eth_caps=`` snap_caps=`` hello_ms=`` status_sent_ms=`` status_seen_ms=`` disconnect_ms=`0x6` local_status=`` remote_status=``
- peer=`7d1e976f507518cd895e5721f4c58a5b8ee446eb04aab7b386ce5c84cc7116d10b32c1e749d84834ffd17bd5f9928ecbe56b1dd206172d24e8ba479ba1590cd6` endpoint=`13.222.122.244:30304` stage=`disconnected` best=`disconnected` class=`endpoint_timeout` phase=`` reason=`connect_failed(13.222.122.244:30304):connection timed out` cap=`none` client=`` eth_caps=`` snap_caps=`` hello_ms=`` status_sent_ms=`` status_seen_ms=`` disconnect_ms=`` local_status=`` remote_status=``
- peer=`b7148466c8558f57da7a16259edcaece6832400c0baaba01b4e20e60c426922791899525f217a6ffb301d1c2b2a2695963b78c5e765f85e84084ee8d2f86db7c` endpoint=`95.216.12.50:30303` stage=`disconnected` best=`status_sent` class=`eth68_only_after_local_status_before_remote_status` phase=`after_status_sent_before_status_seen` reason=`rlpx_remote_disconnected_after_status_sent_before_status_seen:reason_code=4 reason=too_many_peers` cap=`eth/68` client=`erigon/v3.2.2-bc54d33c/linux-amd64/go1.24.3` eth_caps=`68` snap_caps=`` hello_ms=`0x4` status_sent_ms=`0x5` status_seen_ms=`` disconnect_ms=`0x1e2` local_status=`proto=68,network=1,earliest=0,latest=0,fork=0xfc64ec04:1150000,genesis=0xd4e56740f876aef8...,head=0xd4e56740f876aef8...` remote_status=``
- peer=`b7148466c8558f57da7a16259edcaece6832400c0baaba01b4e20e60c426922791899525f217a6ffb301d1c2b2a2695963b78c5e765f85e84084ee8d2f86db7c` endpoint=`95.216.12.50:30304` stage=`disconnected` best=`disconnected` class=`endpoint_timeout` phase=`` reason=`connect_failed(95.216.12.50:30304):connection timed out` cap=`none` client=`` eth_caps=`` snap_caps=`` hello_ms=`` status_sent_ms=`` status_seen_ms=`` disconnect_ms=`` local_status=`` remote_status=``

Public Session Result:
- Local controlled geth session was not exercised because no local enode was supplied.
- Remote controlled geth session was not exercised because no remote controlled enode was supplied.
- Public DNS ENR discovery produced candidate session peers; bootnode/DNS discovery is not treated as eth session readiness.
- Public discovered-peer session reached Hello but did not observe Status in this run.

Status Gap Diagnostics:
- disconnect_before_hello_count: `3`
- disconnect_before_status_count: `0`
- disconnect_after_status_sent_count: `1`
- disconnect_after_hello_before_local_status_count: `0`
- disconnect_after_local_status_before_remote_status_count: `1`
- capability_mismatch_count: `0`
- eth68_only_peer_count: `1`
- eth69_70_peer_count: `0`
- status_payload_mismatch_count: `0`
- endpoint_timeout_count: `4`
- hello_seen_count: `1`
- status_sent_count: `1`
- status_seen_count: `0`
- The sampled public run observed at least one remote Hello but did not observe remote Status.
- Observed Hello samples were eth/68-only and are separated from the eth/69 or eth/70 readiness target.
- Per-peer compact traces include remote client, eth/snap capability hints, disconnect phase, and elapsed timing when the gateway observed them.

Readiness Claim:
- public RLPx readiness: NOT CLAIMED.
- A readiness claim requires TCP success, auth ack, Hello, Status, selected eth/69 or eth/70, and ready_count >= 1 in the public discovered-peer session.

Interpretation:
- Prior local controlled geth evidence passed through TCP, auth ack, Hello, Status, eth/69, and ready.
- A remote controlled geth pass would demonstrate that the gateway RLPx path can traverse a public network path to a known geth peer; random public discovered-peer readiness remains a separate claim.
- If the public session reaches auth ack or Hello but not Status, the likely area is public peer selection, remote peer policy, endpoint quality, or Status exchange compatibility with sampled public peers.
- If a future public run stops before ack, the likely area remains public peer selection, endpoint reachability, network egress, or remote policy.
- If both local and public sessions stop before ack, the next independent patch should inspect RLPx auth/session details.
- A run that does not observe ack also does not proceed far enough to observe Hello, Status, or eth capability negotiation in that run.
- This does not mean the NOVOVM EVM plugin lacks Hello/Status handling.

Not Claimed:
- no full geth full node parity
- no EVM execution semantic rewrite
- no full eth/71 or BAL implementation
- no real balHash metadata source
- no public random-peer readiness unless separately observed
- no old UnifiedAccountRouter state migration
- no strategy-specific acceptance result
- no new NOVOVM plugin architecture

Diff Audit:
- Rust scope: no new Rust changes are required for this follow-up; the report reuses the gateway RLPx Status diagnostics added by the prior Status exchange diagnostics patches.
- Script scope: `scripts/migration/run_evm_rlpx_layered_canary.ps1` adds a RemoteControlledGethEnode comparison layer and reports public Status exchange failure classes.
- Report scope: `artifacts\migration\remote-controlled-geth-rlpx-canary-after-a484a8506.md` records this public Status failure classification canary run.
- RLPx Status timing is not changed by this follow-up.
- No eth_baseFee, balHash, eth/71 guard, BAL fallback, UA RocksDB, or plugin architecture behavior is changed.

Merge Note:
- This is a public RLPx Status exchange failure classification and remote controlled geth evidence patch.
- The observed public run reached auth ack and Hello on sampled peers but did not observe Status or ready.
- Public RLPx readiness remains not claimed until a public discovered-peer session observes Status and ready_count >= 1.
