# NOVOVM netwatch dependency feature patch

Base: crates.io netwatch 0.19.3, upstream n0-computer/net-tools commit
0030e57fc70981895bb86212ff0f059cc41c8a4e (package path netwatch).
Registry archive SHA256:
`39da9cad5f23aa43401f09497d3b280e51b5feba115d9ffbf38ca35d6ff95e96`.
MIT and Apache-2.0 license texts are retained from that exact upstream commit.
The original Rust sources, build script and tests are retained without changes.
Cargo.toml is based on the upstream Cargo.toml.orig, with inherited lint settings
expanded from the published package's normalized manifest for standalone use.

Only the Windows WMI dependency changes: `default-features = false` removes the
unused chrono datetime conversion feature. `interfaces/windows.rs` continues to
use the original WMIConnection, filtered Win32_IP4RouteTable query and blocking
COM task; its deserialized value is a String, not a WMI datetime. Windows route
and interface observation are not disabled or replaced. WMI's version and API,
Iroh's version, other platform implementations and NOVONET transport priority
are unchanged. No advisory or security policy is suppressed.

The existing time 0.3 dependency remains. This patch does not provide a new clock,
network service, protocol, or route selector. Remove the patch only when a
qualified upstream version provides equivalent chrono-free features and retains
the project's independently patched passive Windows netdev behavior.
