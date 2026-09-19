# Standalone relay deployment

`novovm-relay` builds `supervm-relay` from the existing
`novovm-node/src/product_relay_daemon.rs` and `product_relay_client.rs`.
It does not link the consensus, AOEM or RocksDB execution dependencies. The
original node binary remains available; wire formats and authentication are
shared, not reimplemented. This is a relay service, not a complete mainnet node.

Build and test on the target platform:

```sh
cargo build --locked --release -p novovm-relay
cargo test --locked --release -p novovm-relay
```

The binary takes exactly the existing relay JSON configuration path. The service
and bounded sample configuration are under `config/relay/`. Install the binary
at `/opt/supervm/bin/supervm-relay`, configuration at `/etc/supervm/relay.json`,
and the public certificate, private TLS key and existing 32-byte Ed25519 identity
hex file at the paths in `LoadCredential`. Private files must remain root-owned
and mode 0600. systemd provides them to the dynamic service user through its
credential directory; do not copy keys into this repository or runtime reports.

The sample listens on loopback only. A public listener or authenticated TLS
reverse-proxy route requires explicit operator network configuration. Do not
assume a successful process start means cloud ingress or mobile reachability.
The advertised peer identity must match the configured persistent key. Clients
still authenticate that identity independently of transport certificate trust.

Before enabling the unit, validate the configuration with a bounded foreground
smoke run using test-only credentials, then verify the live certificate and
peer identity from a client. Preserve the previous binary/configuration for
rollback. Startup does not apply firewall rules, register DNS, modify consensus,
or enroll the machine into a blockchain.

## Validation (2026-09-20)

Windows `cargo check -p novovm-relay --offline` and Linux release build passed.
All 29 existing relay daemon/client tests passed in the standalone Linux target.
Two timeout-restoration assertions now compare against the effective OS socket
baseline: Linux rounded the requested 10ms to 12ms. Protocol deadlines and
runtime code were not changed.

The 6,296,112-byte Linux executable also passed bounded TLS startup/shutdown on
the deployment host, including the sample queue/connection limits. Those smoke
runs used loopback and test-only credentials and have exited. No permanent
public relay, firewall opening or mobile public route is claimed by these tests.
