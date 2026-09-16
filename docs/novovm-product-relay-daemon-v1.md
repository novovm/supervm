# NOVOVM Product Relay Daemon v1

`novovm-product-relay` is a headless WSS relay runtime. It does not execute
payloads, interpret business semantics, or act as a NOVOVM trust authority.

The relay requires a PEM TLS certificate/key for the WebSocket transport and a
separate Ed25519 relay identity key. The TLS certificate encrypts the transport;
the signed NOVOVM node challenge-response remains the protocol identity check.
The runtime report emitted by this release carries `daemon_version: 2`. Existing
Rust `V1` names are type/scope labels only; they do not promise source, wire, or
report-schema compatibility with an older daemon.

## Configuration

```json
{
  "bind_addr": "0.0.0.0:443",
  "tls_cert_path": "/etc/novovm/tls/fullchain.pem",
  "tls_key_path": "/etc/novovm/tls/privkey.pem",
  "relay_identity_key_path": "/etc/novovm/relay-ed25519.hex",
  "report_path": "/var/lib/novovm/reports/relay.json",
  "report_interval_ms": 5000,
  "max_connections": 512,
  "handshake_timeout_ms": 5000,
  "max_sessions": 256,
  "max_tracked_sources": 1024,
  "session_queue_capacity": 256,
  "session_queue_bytes": 8388608,
  "active_queue_total": 16384,
  "active_queue_bytes_total": 268435456,
  "offline_queue_per_peer": 512,
  "offline_queue_bytes_per_peer": 16777216,
  "offline_queue_per_source": 1024,
  "offline_queue_bytes_per_source": 33554432,
  "offline_queue_total": 16384,
  "offline_queue_bytes_total": 268435456,
  "offline_queue_ttl_ms": 60000,
  "session_ttl_ms": 45000,
  "rate_limit_frames": 4096,
  "max_frames_per_window": 65536,
  "rate_limit_window_ms": 1000,
  "source_bytes_per_minute": 67108864,
  "max_bytes_per_minute": 1073741824
}
```

`max_connections` 是 TLS/HTTP/认证前后的物理连接总上限，且必须大于
`max_sessions`，使 authenticated session 自身不会占满所有物理连接 slot。这是正常连接
headroom，不是 identity-aware replacement reservation；慢速未认证连接仍可占满物理上限，
公网部署仍需要上游 DDoS/连接准入保护。省略 `max_connections` 时，daemon 会取
`max(512, max_sessions + 1)`，保证升级前自定义的大 `max_sessions` 配置不会因新增字段而
启动失败；显式配置仍必须严格大于 `max_sessions`。TLS、HTTP upgrade 和 signed offer
共用 `handshake_timeout_ms` 绝对时限。active 队列受 count/bytes 约束；offline 队列另受
count/bytes/TTL 约束；data 与 peer-handshake control 消息不能分别重复获得一份额度。
active 的进程级上限由 `active_queue_total` / `active_queue_bytes_total` 独立控制，不能再由
`max_sessions * session_queue_*` 放大。被替换、断开或过期 session 的旧 inbox 若仍真实持有
payload，会继续占用全局额度，直到该 item 被读取或 inbox 被销毁；重连不能清零这笔内存账。

`source_bytes_per_minute` 与 `max_bytes_per_minute` 统计认证后的 relay ingress wire-message
bytes，使用固定 60 秒窗口。它们不包含 relay egress、WebSocket framing、TLS 或 TCP 开销，
因此 signed directory 的容量承诺与 daemon 配置必须按这个口径保持一致。
`rate_limit_frames` 是单 identity 窗口上限，`max_frames_per_window` 是所有 identity 共享的
同窗口上限；后者关闭大量小 frame 的 aggregate CPU 绕过。

`relay_identity_key_path` contains exactly 64 hexadecimal characters: a single
32-byte Ed25519 secret. It must be readable only by the relay service account
and must never be placed in the JSON configuration, report, manifest, or relay
record.

Run the daemon with:

```bash
novovm-product-relay /etc/novovm/relay.json
```

For a bounded smoke run only, add `"run_for_ms": 60000` to the config. Normal
deployments omit it and manage process lifetime through the operating system.

Embedded callers can use `run_product_relay_daemon_with_shutdown_v1(config, stopping)`
with a shared `Arc<AtomicBool>` and their own thread handle. Set the signal to true and
join the thread when the owning scope ends; do not reset it during a run. The original
entry point remains available. Configured run duration uses a monotonic clock.
External stop, duration expiry and accept/spawn/report errors all close the listener,
request graceful session shutdown and join connection workers before returning.
This library API does not add OS signal handling to the standalone executable.

## Runtime Boundary

- Client WebSocket frames must use a fresh unpredictable mask and are limited to 1 MiB;
  control frames are limited to 125 bytes. The same 1 MiB limit applies before relay writes.
- The HTTP upgrade requires the exact product path, HTTP/1.1, Host, Upgrade/Connection tokens,
  WebSocket version 13, and one base64 key decoding to 16 bytes.
- A client first sends a signed NOVOVM handshake offer; no arbitrary `peer_id`
  registration is accepted.
- After relay-session authentication, the relay can forward signed peer handshake
  offers and responses by `target_peer_id`. This establishes an A/B E2E session
  without requiring either NAT node to expose an inbound listener. The relay only
  checks that the signed message route matches its authenticated source session.
- Relay sessions are authenticated, expiring, rate-limited, bounded, replaceable
  on reconnect, and closed during graceful shutdown.
- `Data` and peer-handshake requests receive a correlated `ForwardOutcome`. An accepted
  outcome means the relay took bounded in-memory ownership or forwarded to the active
  target session; it is not a recipient ACK or durable persistence receipt.
- Authenticated sender requests and their outcomes have priority. After an outcome/ACK, or a
  Ping/Pong, the daemon fairly emits at most one alternating data/control delivery. After an idle
  read timeout it may additionally drain at most 11 data and four peer-handshake deliveries.
- TLS/HTTP/signed-handshake and authenticated frame/read-response-write operations have absolute
  lower-TCP deadlines, so TLS-record slow drip and blocked writes cannot renew progress forever.
- Raw authenticated bytes are frame/byte admitted before JSON decode. A predecode budget or
  malformed-frame rejection closes the connection because trustworthy correlation fields do not
  yet exist; decoded forwards receive correlated outcomes.
- Only `SecureNovoRudpEnvelopeV1` ciphertext is forwarded. The relay cannot
  decrypt its NOVORUDP frame or infer execution semantics.
- `reports/relay.json` is atomically replaced with session, bounded-limit, queue, byte, rate and
  rejection counters. Evidence admission requires a self-consistent daemon report version 2.
- WebPKI/CA trust is not a NOVOVM identity root. A client must bind the relay
  node key through a signed relay record and verify the node handshake.

`ForwardOutcome` changes the v1 relay wire contract. This release does not negotiate relay
protocol capabilities during handshake, so daemon and clients must be upgraded as one
homogeneous deployment. Mixed old/new relay processes are not rolling-upgrade compatible.

## 2026-09-16 idle relay file recovery

The KINGCLUB native integration test reproduced timeout after closing the receiver UDP socket mid-file while retaining its authenticated WSS session. The former four-data-frame idle tick accumulated retransmission backlog. Use the existing client pending-byte budget to derive the data allowance: 11 maximum-sized data frames plus four reserved control frames, below both the 64-event and 16 MiB client limits. Admission, payload size, authentication and shutdown checks are unchanged.

Validation: all 12 product_relay_daemon tests passed; rebuilt local daemon. Actual native AEAD/UDP/WSS tests passed in relay, LAN, and LAN-interrupted modes plus endpoint recovery. Interrupted transfer delivered and hash-verified the same 262161-byte file after a deliberately lost final receipt (14.298 s on this computer). This is not handset, WAN, throughput, or production-scale acceptance.

## 2026-09-16 scoped lifecycle regression

The Linux CI run for `5ac0f0e` failed in the three-node mesh E2E wait. Its logs do
not identify whether this was C's initial connection or restart. A controlled local
regression delays C's restart by 8.5 seconds: the old eight-second daemon duration
expires first and reproduces an empty peer set at restart. Owning the daemon until
scenario completion fixes that regression without increasing per-operation assertions
or the mesh session TTL. Phase and elapsed-time diagnostics now distinguish initial
connections, restart and post-restart delivery.

Windows validation: 28 relay client/daemon tests passed; all 21 mainline Overlay tests
passed three consecutive default-parallel runs and one serial run. Tests include real
TLS/HTTP/WebSocket split input delayed beyond the idle poll, fixed absolute deadlines,
partial-frame expiry across outbound heartbeats, idle-handshake shutdown and report-error
cleanup. These are local regressions, not a multi-device, public-network or finality sign-off.
