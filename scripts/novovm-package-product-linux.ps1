param(
  [string]$Target = "x86_64-unknown-linux-gnu",
  [string]$PackageDir = "artifacts\product-overlay\linux-x86_64",
  [switch]$Force
)

$ErrorActionPreference = "Stop"
$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $repo
try {
  $installedTargets = @(rustup target list --installed)
  if ($installedTargets -notcontains $Target) {
    throw "Rust target '$Target' is not installed. Install it with: rustup target add $Target. A Linux package was not generated."
  }

  $packagePath = Join-Path $repo $PackageDir
  if (Test-Path $packagePath) {
    if (!$Force) {
      throw "Package directory already exists: $packagePath. Use -Force only for an intentional replacement."
    }
    Remove-Item -LiteralPath $packagePath -Recurse -Force
  }

  $bins = @(
    "novovm-node",
    "novovm-product-relay",
    "novovm-product-node-overlay",
    "novovm-product-nat",
    "novovm-product-peer",
    "novovm-product-topology",
    "novovm-product-evidence"
  )
  foreach ($bin in $bins) {
    cargo build -q -p novovm-node --release --target $Target --bin $bin
  }

  New-Item -ItemType Directory -Force -Path (Join-Path $packagePath "bin") | Out-Null
  New-Item -ItemType Directory -Force -Path (Join-Path $packagePath "config") | Out-Null
  New-Item -ItemType Directory -Force -Path (Join-Path $packagePath "reports") | Out-Null
  New-Item -ItemType Directory -Force -Path (Join-Path $packagePath "systemd") | Out-Null
  New-Item -ItemType Directory -Force -Path (Join-Path $packagePath "docs") | Out-Null
  New-Item -ItemType Directory -Force -Path (Join-Path $packagePath "aoem") | Out-Null
  $packageRoot = (Resolve-Path $packagePath).Path

  foreach ($bin in $bins) {
    $source = Join-Path $repo "target\$Target\release\$bin"
    if (!(Test-Path $source)) { throw "Expected Linux binary was not built: $source" }
    Copy-Item -Force $source (Join-Path $packagePath "bin\$bin")
  }

  $aoemRoot = Join-Path $repo "aoem"
  $aoemLinux = Join-Path $aoemRoot "linux"
  $aoemCore = Join-Path $aoemLinux "core\bin\libaoem_ffi.so"
  $aoemManifest = Join-Path $aoemRoot "manifest\aoem-manifest.json"
  $aoemProfile = Join-Path $aoemRoot "config\aoem-runtime-profile.json"
  foreach ($required in @($aoemLinux, $aoemCore, $aoemManifest, $aoemProfile)) {
    if (!(Test-Path -LiteralPath $required)) {
      throw "Required bundled AOEM FULLMAX Linux runtime path is missing: $required"
    }
  }
  $linuxManifest = Get-Content -Raw -LiteralPath (Join-Path $aoemLinux "manifest.json") | ConvertFrom-Json
  if ($linuxManifest.profile -ne "fullmax" -or $linuxManifest.platform -ne "linux") {
    throw "Bundled AOEM Linux runtime is not a Linux FULLMAX package."
  }
  $expectedAoemSha = (
    Get-Content -Raw -LiteralPath (Join-Path $aoemRoot "aoem-sdk-manifest.json") |
      ConvertFrom-Json
  ).platforms.'linux-x86_64'.library_sha256
  $actualAoemSha = (Get-FileHash -LiteralPath $aoemCore -Algorithm SHA256).Hash.ToLowerInvariant()
  if ([string]::IsNullOrWhiteSpace($expectedAoemSha) -or $actualAoemSha -ne $expectedAoemSha.ToLowerInvariant()) {
    throw "Bundled AOEM FULLMAX Linux core checksum does not match the SDK manifest."
  }
  Copy-Item -LiteralPath $aoemLinux -Destination (Join-Path $packagePath "aoem") -Recurse
  Copy-Item -LiteralPath (Join-Path $aoemRoot "manifest") -Destination (Join-Path $packagePath "aoem") -Recurse
  Copy-Item -LiteralPath (Join-Path $aoemRoot "config") -Destination (Join-Path $packagePath "aoem") -Recurse
  foreach ($metadata in @(
    "aoem-sdk-manifest.json",
    "INSTALL-INFO.txt",
    "README.md",
    "RELEASE-NOTES.md",
    "RUNTIME-BASELINE.md"
  )) {
    Copy-Item -LiteralPath (Join-Path $aoemRoot $metadata) -Destination (Join-Path $packagePath "aoem\$metadata")
  }

  @'
{
  "bind_addr": "0.0.0.0:443",
  "tls_cert_path": "/etc/novovm/tls/fullchain.pem",
  "tls_key_path": "/etc/novovm/tls/privkey.pem",
  "relay_identity_key_path": "/etc/novovm/relay-ed25519.hex",
  "report_path": "/var/lib/novovm/reports/relay.json",
  "report_interval_ms": 5000
}
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "config\relay.json.example")

  @'
{
  "role": "sender",
  "identity_key_path": "/etc/novovm/peer-a-ed25519.hex",
  "relay": {
    "endpoint": "wss://relay.example/novovm",
    "expected_relay_peer_id": "<verified-relay-peer-id>",
    "tls_trust": "native_web_pki"
  },
  "target_peer_id": "<peer-b-id>",
  "payload_paths": ["/var/lib/novovm/outbound/operator-provided-payload.bin"],
  "report_path": "/var/lib/novovm/reports/peer-a.json"
}
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "config\peer-sender.json.example")

  @'
{
  "role": "receiver",
  "identity_key_path": "/etc/novovm/peer-b-ed25519.hex",
  "relay": {
    "endpoint": "wss://relay.example/novovm",
    "expected_relay_peer_id": "<verified-relay-peer-id>",
    "tls_trust": "native_web_pki"
  },
  "expected_source_peer_id": "<peer-a-id>",
  "expected_frame_count": 1,
  "report_path": "/var/lib/novovm/reports/peer-b.json"
}
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "config\peer-receiver.json.example")

  @'
{
  "chain_id": 1,
  "role": "duplex",
  "identity_key_path": "/etc/novovm/node-ed25519.hex",
  "peers": [
    {
      "peer_id": "novovm-ed25519:<peer-b-public-key>",
      "metric_peer_id": 9991002
    },
    {
      "peer_id": "novovm-ed25519:<peer-c-public-key>",
      "metric_peer_id": 9991003
    },
    {
      "peer_id": "novovm-ed25519:<peer-d-public-key>",
      "metric_peer_id": 9991004
    }
  ],
  "overlay": {
    "cache_path": "/var/lib/novovm/bootstrap-cache.json",
    "trusted_signer_public_keys": [[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]],
    "minimum_bootstrap_signatures": 1,
    "embedded_sources": [],
    "cooldown_base_ms": 2000,
    "cooldown_max_ms": 300000
  },
  "connect_timeout_ms": 10000,
  "read_timeout_ms": 250,
  "tls_trust": "native_web_pki",
  "channel_capacity": 4096,
  "metric_peer_id": 9991000,
  "reconnect_base_delay_ms": 250,
  "reconnect_max_delay_ms": 30000
}
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "config\node-mainline-overlay.json.example")

  @'
NOVOVM_NODE_MODE=native_execution_pipeline
NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED=true
NOVOVM_PRODUCT_MAINLINE_OVERLAY_CONFIG=/etc/novovm/node-mainline-overlay.json
NOVOVM_PRODUCT_MAINLINE_OVERLAY_MAX_PER_TICK=1024
NOVOVM_PRODUCT_MAINLINE_OVERLAY_MAX_PROPAGATIONS=3
NOVOVM_PRODUCT_MAINLINE_OVERLAY_EVENT_BUDGET=4096
NOVOVM_AOEM_VARIANT=core
NOVOVM_AOEM_ROOT=/opt/novovm/aoem
NOVOVM_AOEM_DLL=/opt/novovm/aoem/linux/core/bin/libaoem_ffi.so
NOVOVM_AOEM_MANIFEST=/opt/novovm/aoem/manifest/aoem-manifest.json
NOVOVM_AOEM_RUNTIME_PROFILE=/opt/novovm/aoem/config/aoem-runtime-profile.json
NOVOVM_AOEM_PLUGIN_DIR=/opt/novovm/aoem/linux/core/plugins
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "config\novovm-node.env.example")

  @'
{
  "scope": "novovm_product_mainline_topology_plan_v1",
  "chain_id": 1,
  "require_identity_files": false,
  "nodes": [
    {
      "name": "node-a",
      "peer_id": "novovm-ed25519:<node-a-public-key>",
      "config_path": "node-a.json"
    },
    {
      "name": "node-b",
      "peer_id": "novovm-ed25519:<node-b-public-key>",
      "config_path": "node-b.json"
    },
    {
      "name": "node-c",
      "peer_id": "novovm-ed25519:<node-c-public-key>",
      "config_path": "node-c.json"
    },
    {
      "name": "node-d",
      "peer_id": "novovm-ed25519:<node-d-public-key>",
      "config_path": "node-d.json"
    }
  ]
}
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "config\topology-plan.json.example")

  @'
[Unit]
Description=NOVOVM Product WSS Relay
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=novovm
Group=novovm
ExecStart=/opt/novovm/bin/novovm-product-relay /etc/novovm/relay.json
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/novovm

[Install]
WantedBy=multi-user.target
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "systemd\novovm-product-relay.service")

  @'
[Unit]
Description=NOVOVM Main Node with Product Overlay
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=novovm
Group=novovm
EnvironmentFile=/etc/novovm/novovm-node.env
ExecStart=/opt/novovm/bin/novovm-node
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/novovm

[Install]
WantedBy=multi-user.target
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "systemd\novovm-node.service")

  Copy-Item -Force (Join-Path $repo "docs\NOVOVM_PRODUCT_MAINLINE_OVERLAY_LIFECYCLE_V1.md") (Join-Path $packagePath "docs\")
  Copy-Item -Force (Join-Path $repo "docs\novovm-product-topology-preflight-v1.md") (Join-Path $packagePath "docs\")
  Copy-Item -Force (Join-Path $repo "docs\novovm-product-relay-daemon-v1.md") (Join-Path $packagePath "docs\")
  Copy-Item -Force (Join-Path $repo "docs\novovm-product-node-overlay-v1.md") (Join-Path $packagePath "docs\")
  Copy-Item -Force (Join-Path $repo "docs\novovm-product-nat-runtime-v1.md") (Join-Path $packagePath "docs\")
  Copy-Item -Force (Join-Path $repo "docs\novovm-product-peer-runtime-v1.md") (Join-Path $packagePath "docs\")
  Copy-Item -Force (Join-Path $repo "docs\novovm-product-evidence-v1.md") (Join-Path $packagePath "docs\")
  Copy-Item -Force (Join-Path $repo "docs\novovm-product-relay-client-v1.md") (Join-Path $packagePath "docs\")

  @'
# NOVOVM Product Overlay Linux Package

This package is a headless runtime bundle. It contains no Rust toolchain, IDE,
Codex installation, or source workspace.

1. Verify `CHECKSUMS.sha256` before deployment.
2. Install the binaries under `/opt/novovm/bin` and configure `/etc/novovm`.
3. Keep relay/node Ed25519 secret files readable only by the service account.
4. For a relay, use the included relay systemd unit after reviewing paths and user.
5. For a main node, install `node-mainline-overlay.json` and `novovm-node.env`,
   then review the included main-node systemd unit.
6. Keep the bundled generic AOEM FULLMAX runtime under `/opt/novovm/aoem`;
   `novovm-node.env` pins its core, manifest, profile, and sidecar paths.
7. Run `novovm-product-topology` before deployment and preserve its offline
   preflight report. It never claims that an external topology was executed.
8. Generate a signed post-run evidence manifest with `novovm-product-evidence`.

TLS protects the WSS transport. NOVOVM node challenge-response remains the
protocol identity check; a CA is not the NOVOVM trust root.

AOEM is a bundled third-party, domain-neutral execution engine. NOVOVM
authentication, transaction semantics, overlay routing, and product policy
remain in the host; no NOVOVM-specific business logic is added to AOEM.
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "README.md")

  $entries = Get-ChildItem -LiteralPath $packagePath -File -Recurse |
    Where-Object { $_.Name -ne "CHECKSUMS.sha256" -and $_.Name -ne "release-manifest.json" } |
    ForEach-Object {
      $relative = $_.FullName.Substring($packageRoot.Length).TrimStart([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar).Replace([IO.Path]::DirectorySeparatorChar, [char]'/')
      $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
      [PSCustomObject]@{ path = $relative; sha256 = $hash; bytes = $_.Length }
    } | Sort-Object path
  $entries | ForEach-Object { "$($_.sha256)  $($_.path)" } | Set-Content -Encoding ascii (Join-Path $packagePath "CHECKSUMS.sha256")
  $commit = (git rev-parse HEAD).Trim()
  [PSCustomObject]@{
    scope = "novovm_product_overlay_linux_release_v1"
    target = $Target
    git_commit = $commit
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    signed_evidence_included = $false
    note = "Generate signed runtime evidence after deployment; this build manifest is checksum-only."
    artifacts = $entries
  } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $packagePath "release-manifest.json")
  Write-Host "Linux product overlay package created: $packagePath"
}
finally {
  Pop-Location
}
