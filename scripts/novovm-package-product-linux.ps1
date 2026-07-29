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
    "novovm-product-relay",
    "novovm-product-node-overlay",
    "novovm-product-nat",
    "novovm-product-peer",
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
  $packageRoot = (Resolve-Path $packagePath).Path

  foreach ($bin in $bins) {
    $source = Join-Path $repo "target\$Target\release\$bin"
    if (!(Test-Path $source)) { throw "Expected Linux binary was not built: $source" }
    Copy-Item -Force $source (Join-Path $packagePath "bin\$bin")
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
4. For a relay, use the included systemd unit after reviewing paths and user.
5. Generate a signed post-run evidence manifest with `novovm-product-evidence`.

TLS protects the WSS transport. NOVOVM node challenge-response remains the
protocol identity check; a CA is not the NOVOVM trust root.
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
