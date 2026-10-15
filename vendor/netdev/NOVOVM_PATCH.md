# NOVOVM vendored netdev patch

Base: crates.io `netdev` 0.45.1, MIT, shellrow/netdev. Package checksum:
`096c44b66a9b09b99b4dd36b1c295ff3a492039ccecaf55466bd6ddcdba59968`.
The registry package
records upstream commit `eea23d5b17d49202eefee2e795cfedcfaa4aac3b`.
`LICENSE` is preserved unchanged. Source, examples, manifests, build script and
README are retained; registry metadata, build outputs and package lock are not.

## Windows passive interface snapshot (2026-10-16)

The Windows 0.45.1 interface enumerator called synchronous `SendARP` for each
active IPv4 gateway while collecting interface state. Iroh/netwatch awaits this
snapshot during Endpoint startup. An unresponsive or virtual gateway therefore
made a local metadata query depend on active network resolution.

This patch changes only `src/os/windows/interface.rs` and enables
`Win32_System_LibraryLoader` in the two package manifests:

- Reuse the family-sized socket address parser, optional `GetIpNetEntry2`
  dynamic lookup, cached-neighbor MAC reader and gateway-address collector from
  netdev 0.46.3 (registry upstream commit
  `4505d238696975a97b029c1b8607487453c0f86e`, same MIT license).
- Read an existing neighbor-cache entry only. Do not call `SendARP` or actively
  resolve a missing MAC. Missing API/cache data remains unknown.
- Keep reported IPv4/IPv6 gateway addresses even if their MAC is unknown;
  absence of a resolved MAC no longer drops the entire gateway.
- Preserve the adapter's own MAC, interface flags, addresses, DNS, statistics,
  default-interface selection and all non-Windows source unchanged.
- Add focused tests for unknown-MAC address preservation, cached-MAC
  preservation, duplicate addresses and absent gateways, plus family-sized
  parsing, IPv6 scope preservation and malformed-length rejection.

An upgrade to unmodified 0.46.3 alone is insufficient: that version still calls
`SendARP` after a cache miss. This is not an authentication or route-trust
change, and a cached MAC is not a security identity. Windows WMI default-route
querying in netwatch is separate and is not modified here.

Validation is run by the SUPERVM integration owner. A source patch or local
unit test is not Android, public-network latency, or production acceptance.

The root workspace excludes this vendor package for standalone unit tests;
its generated standalone Cargo.lock/target are ignored. Production consumers
resolve the path patch through their own reviewed workspace lockfile.

Imported README trailing whitespace is normalized; license text is unchanged.
