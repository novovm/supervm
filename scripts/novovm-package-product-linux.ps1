param(
  [string]$Target = "x86_64-unknown-linux-gnu",
  [string]$PackageDir = "artifacts\product-overlay\linux-x86_64",
  [switch]$Force
)

$ErrorActionPreference = "Stop"

function Assert-PlainPackagePath([string]$Path, [switch]$Tree, [switch]$AllowFileHardLink) {
  # Reject redirects before traversing them, including the output directory's
  # ancestors. Do not use recursive enumeration, which can follow links.
  $leaf = [IO.Path]::GetFullPath($Path)
  $cursor = $leaf
  $ancestors = [Collections.Generic.Stack[string]]::new()
  while ($cursor) {
    $ancestors.Push($cursor)
    $parent = [IO.Path]::GetDirectoryName($cursor)
    if ($parent -eq $cursor) { break }
    $cursor = $parent
  }
  while ($ancestors.Count) {
    $cursor = $ancestors.Pop()
    $item = $null
    try { $item = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop }
    catch [System.Management.Automation.ItemNotFoundException] { }
    if ($item) {
      # Cargo may hardlink its reported ELF to release/deps. Only that explicit
      # file leaf may share an inode; ancestors and package trees stay strict.
      $allowedHardLink = $AllowFileHardLink -and $cursor -eq $leaf -and
        !$item.PSIsContainer -and $item.LinkType -eq 'HardLink'
      if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
          ($item.LinkType -and !$allowedHardLink)) {
        throw "Package paths cannot traverse links/reparse points: $cursor"
      }
    }
  }
  if ($Tree -and (Test-Path -LiteralPath $Path)) {
    $pending = [Collections.Generic.Queue[string]]::new()
    $pending.Enqueue($Path)
    while ($pending.Count) {
      foreach ($item in Get-ChildItem -LiteralPath $pending.Dequeue() -Force) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $item.LinkType) {
          throw "Package trees cannot contain links/reparse points: $($item.FullName)"
        }
        if ($item.PSIsContainer) { $pending.Enqueue($item.FullName) }
      }
    }
  }
}

function Resolve-PackageDestination([string]$Repository, [string]$Directory) {
  if ([string]::IsNullOrWhiteSpace($Directory)) { throw "PackageDir cannot be empty." }
  $outputRoot = [IO.Path]::GetFullPath((Join-Path $Repository "artifacts/product-overlay"))
  $candidate = if ([IO.Path]::IsPathRooted($Directory)) { $Directory } else { Join-Path $Repository $Directory }
  $candidate = [IO.Path]::GetFullPath($candidate).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
  $comparison = if ([IO.Path]::DirectorySeparatorChar -eq '\') { [StringComparison]::OrdinalIgnoreCase } else { [StringComparison]::Ordinal }
  # A single named package below the dedicated root, never the root or an
  # arbitrary subtree such as source, a database, or a sibling repository.
  if (![string]::Equals([IO.Path]::GetDirectoryName($candidate), $outputRoot, $comparison) -or
      [IO.Path]::GetFileName($candidate) -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') {
    throw "PackageDir must name one package directly below $outputRoot"
  }
  Assert-PlainPackagePath $candidate -Tree
  if ((Test-Path -LiteralPath $candidate) -and !(Test-Path -LiteralPath $candidate -PathType Container)) {
    throw "Package destination is not a directory: $candidate"
  }
  return $candidate
}

function Get-CargoExecutable([object[]]$Messages, [string]$Binary) {
  $artifacts = @($Messages | ForEach-Object {
    $message = $_ | ConvertFrom-Json
    if ($message.reason -eq 'compiler-artifact' -and $message.target.name -eq $Binary -and
        $message.target.kind -contains 'bin' -and $message.executable) { $message.executable }
  } | Select-Object -Unique)
  if ($artifacts.Count -ne 1) { throw "Cargo did not identify exactly one executable for '$Binary'." }
  $executable = [IO.Path]::GetFullPath($artifacts[0])
  if (!(Test-Path -LiteralPath $executable -PathType Leaf)) { throw "Cargo executable missing: $executable" }
  Assert-PlainPackagePath $executable -AllowFileHardLink
  $stream = [IO.File]::OpenRead($executable)
  try {
    $magic = [byte[]]::new(4)
    if ($stream.Read($magic, 0, 4) -ne 4 -or [BitConverter]::ToString($magic) -ne '7F-45-4C-46') {
      throw "Cargo executable is not an ELF Linux artifact: $executable"
    }
  } finally { $stream.Dispose() }
  return $executable
}

function Publish-LinuxPackage([string]$Repository, [string]$Staged, [string]$Destination, [switch]$Replace) {
  # Publication uses same-directory renames. Never recursively delete an old
  # package; retain it for rollback, even after a successful replacement.
  $Destination = Resolve-PackageDestination $Repository $Destination
  $Staged = Resolve-PackageDestination $Repository $Staged
  if ($Staged -eq $Destination -or !(Test-Path -LiteralPath $Staged -PathType Container)) {
    throw "A distinct completed staging directory is required."
  }
  $previous = $null
  if (Test-Path -LiteralPath $Destination) {
    if (!$Replace) { throw "Package directory already exists: $Destination. Use -Force for intentional replacement." }
    $previous = Resolve-PackageDestination $Repository ($Destination + '.previous.' + [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::Move($Destination, $previous)
  }
  try {
    [IO.Directory]::Move($Staged, $Destination)
  } catch {
    if ($previous -and !(Test-Path -LiteralPath $Destination)) {
      Assert-PlainPackagePath $previous -Tree
      [IO.Directory]::Move($previous, $Destination)
    }
    throw
  }
  if ($previous) { Write-Host "Previous package retained for rollback: $previous" }
}

$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $repo
try {
  $destination = Resolve-PackageDestination $repo $PackageDir
  if ((Test-Path -LiteralPath $destination) -and !$Force) {
    throw "Package directory already exists: $destination. Use -Force only for an intentional replacement."
  }
  if ($Target -ne 'x86_64-unknown-linux-gnu') {
    throw "The bundled AOEM Linux runtime supports x86_64-unknown-linux-gnu only; no compatible runtime is packaged for '$Target'."
  }
  $dirty = @(git status --porcelain=v1 --untracked-files=all)
  if ($LASTEXITCODE -ne 0) {
    throw "Unable to verify the Git worktree before packaging."
  }
  if ($dirty.Count -ne 0) {
    throw "Linux release packaging requires a clean Git worktree so binaries and git_commit describe the same source. Commit or remove local changes first."
  }
  $commit = (git rev-parse HEAD).Trim()
  if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($commit)) {
    throw "Unable to identify the package source commit."
  }

  $installedTargets = @(rustup target list --installed)
  if ($LASTEXITCODE -ne 0) {
    throw "Unable to determine installed Rust targets. A Linux package was not generated."
  }
  if ($installedTargets -notcontains $Target) {
    throw "Rust target '$Target' is not installed. Install it with: rustup target add $Target. A Linux package was not generated."
  }

  $nodeBins = @(
    "novovm-node",
    "novovm-native-client",
    "novovm-product-relay",
    "novovm-product-node-overlay",
    "novovm-product-nat",
    "novovm-product-peer",
    "novovm-product-topology",
    "novovm-product-evidence"
  )
  # One original package, using the real crate/feature contracts. These are
  # service adapters, not another network, account or execution implementation.
  $buildContracts = @($nodeBins | ForEach-Object {
    [PSCustomObject]@{ package = 'novovm-node'; binary = $_; features = @(); default_features = $true }
  }) + @(
    [PSCustomObject]@{ package = 'novovm-relay'; binary = 'novovm-rendezvous-service'; features = @('iroh-transport', 'tls-transport'); default_features = $true },
    [PSCustomObject]@{ package = 'novovm-relay'; binary = 'supervm-relay'; features = @('iroh-transport', 'tls-transport'); default_features = $true },
    [PSCustomObject]@{ package = 'novovmctl'; binary = 'novovmctl'; features = @(); default_features = $true }
  )
  $bins = @($buildContracts | ForEach-Object binary)
  $metadataOutput = @(cargo metadata --locked --no-deps --format-version 1)
  if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed; package identities cannot be verified.' }
  $workspaceMetadata = ($metadataOutput -join "`n") | ConvertFrom-Json
  foreach ($contract in $buildContracts) {
    $packages = @($workspaceMetadata.packages | Where-Object name -eq $contract.package)
    if ($packages.Count -ne 1 -or [string]::IsNullOrWhiteSpace($packages[0].id)) {
      throw "Cargo metadata must identify exactly one package '$($contract.package)'."
    }
    $contract | Add-Member -NotePropertyName expected_cargo_package_id -NotePropertyValue $packages[0].id
  }
  $executables = @{}
  $executableHashes = @{}
  foreach ($contract in $buildContracts) {
    $bin = $contract.binary
    $featureArgs = if ($contract.features.Count) { @('--features', ($contract.features -join ',')) } else { @() }
    $buildMessages = @(cargo build --locked -p $contract.package --release --target $Target --bin $bin @featureArgs --message-format=json-render-diagnostics)
    if ($LASTEXITCODE -ne 0) {
      throw "Linux build failed for '$bin'; refusing to package possibly stale binaries."
    }
    # Cargo reports the actual artifact, including .cargo/config.toml and any
    # CARGO_TARGET_DIR override. Never guess a path or accept an old namesake.
    $executables[$bin] = Get-CargoExecutable $buildMessages $bin
    $executableHashes[$bin] = (Get-FileHash -LiteralPath $executables[$bin] -Algorithm SHA256).Hash
    $artifact = @($buildMessages | ForEach-Object { $_ | ConvertFrom-Json } |
      Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq $bin -and $_.executable })[-1]
    if ([string]::IsNullOrWhiteSpace($artifact.package_id)) { throw "Cargo package identity missing for '$bin'." }
    if ($artifact.package_id -cne $contract.expected_cargo_package_id) {
      throw "Cargo artifact package identity mismatch for '$bin'."
    }
    foreach ($feature in $contract.features) {
      if (@($artifact.features) -notcontains $feature) { throw "Cargo did not enable required feature '$feature' for '$bin'." }
    }
    $contract | Add-Member -NotePropertyName cargo_package_id -NotePropertyValue $artifact.package_id -Force
    $contract | Add-Member -NotePropertyName cargo_features -NotePropertyValue @($artifact.features) -Force
    $contract | Add-Member -NotePropertyName sha256 -NotePropertyValue $executableHashes[$bin].ToLowerInvariant() -Force
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
  $metadataFiles = @(
    "aoem-sdk-manifest.json", "INSTALL-INFO.txt", "README.md",
    "RELEASE-NOTES.md", "RUNTIME-BASELINE.md"
  )
  $packageDocs = @(
    "NOVOVM_NATIVE_OPERATOR_CLIENT.md",
    "NOVOVM_MULTI_HOST_NUAS_HANDOFF.md",
    "NOVOVM_PURPOSE_IDENTITY_PREPARATION.md",
    "NOVOVM_DELIVERY_ALIGNMENT.md",
    "NOVONET_TRANSPORT_POLICY_V1.md",
    "NOVOVM_MAINNET_OPERATOR_PRIMER.zh-CN.txt",
    "NOVOVM_JULY_MAIN_INTEGRATION_MAP.md",
    "NOVOVM_PRODUCT_MAINLINE_OVERLAY_LIFECYCLE_V1.md",
    "NOVOVM_PRODUCT_DURABLE_RECIPIENT_ACK_DELIVERY_JOURNAL_V1.md",
    "novovm-product-topology-preflight-v1.md", "novovm-product-relay-daemon-v1.md",
    "novovm-product-node-overlay-v1.md", "novovm-product-nat-runtime-v1.md",
    "novovm-product-peer-runtime-v1.md", "novovm-product-evidence-v1.md",
    "novovm-product-relay-client-v1.md", "NOVOVM_PRODUCT_RELAY_ADMISSION_RESOURCE_BOUNDS_V1.md",
    "NOVOVM_PRODUCT_OVERLAY_MESH_PEER_ERROR_DOMAIN_V1.md"
  )
  # Existing platform resources are copied without executing or reprovisioning
  # them. In particular B's owner/credential/SQLite identity must remain intact.
  $platformResources = @(
    'config/rendezvous-linux/novonet-mailbox.service',
    'config/rendezvous-linux/README.md',
    'scripts/linux/install-owned-mailbox.sh',
    'config/relay/supervm-relay.service',
    'config/relay/relay.example.json',
    'config/relay/nginx-location.conf'
  )
  foreach ($inputPath in @($metadataFiles | ForEach-Object { Join-Path $aoemRoot $_ }) +
      @($packageDocs | ForEach-Object { Join-Path $repo "docs/$_" }) +
      @($platformResources | ForEach-Object { Join-Path $repo $_ })) {
    if (!(Test-Path -LiteralPath $inputPath -PathType Leaf)) { throw "Package input missing: $inputPath" }
    Assert-PlainPackagePath $inputPath
  }

  # All builds and runtime inputs validated. Generate a separate complete bundle
  # before any existing package is moved. Failed stages are retained, not erased.
  $packagePath = Resolve-PackageDestination $repo ($destination + '.staging.' + [Guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Path $packagePath | Out-Null
  foreach ($directory in @('bin', 'config', 'reports', 'systemd', 'docs', 'aoem', 'compatibility', 'resources')) {
    New-Item -ItemType Directory -Path (Join-Path $packagePath $directory) | Out-Null
  }
  $packageRoot = $packagePath
  foreach ($bin in $bins) {
    Assert-PlainPackagePath $executables[$bin] -AllowFileHardLink
    $copiedExecutable = Join-Path $packagePath "bin/$bin"
    Copy-Item -LiteralPath $executables[$bin] -Destination $copiedExecutable
    Assert-PlainPackagePath $copiedExecutable
    if ((Get-FileHash -LiteralPath $copiedExecutable -Algorithm SHA256).Hash -ne $executableHashes[$bin]) {
      throw "Cargo executable changed before/during copying: $bin; existing package remains untouched."
    }
  }
  Copy-Item -LiteralPath $aoemLinux -Destination (Join-Path $packagePath "aoem") -Recurse
  Copy-Item -LiteralPath (Join-Path $aoemRoot "manifest") -Destination (Join-Path $packagePath "aoem") -Recurse
  Copy-Item -LiteralPath (Join-Path $aoemRoot "config") -Destination (Join-Path $packagePath "aoem") -Recurse
  foreach ($metadata in $metadataFiles) {
    Copy-Item -LiteralPath (Join-Path $aoemRoot $metadata) -Destination (Join-Path $packagePath "aoem\$metadata")
  }
  foreach ($resource in $platformResources) {
    $copiedResource = Join-Path $packagePath "resources/$resource"
    New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($copiedResource)) -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repo $resource) -Destination $copiedResource
  }

  @'
{
  "bind_addr": "127.0.0.1:45172",
  "tls_cert_path": "/etc/novovm/tls/fullchain.pem",
  "tls_key_path": "/etc/novovm/tls/privkey.pem",
  "relay_identity_key_path": "/etc/novovm/relay-ed25519.hex",
  "report_path": "/var/lib/novovm-relay/reports/relay.json",
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
  "delivery_journal_path": "/var/lib/novovm/state/product-delivery-journal.rocksdb",
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
  "channel_capacity": 1024,
  "resource_limits": {
    "pending_per_peer_count": 1024,
    "pending_per_peer_bytes": 67108864,
    "pending_total_count": 16384,
    "pending_total_bytes": 268435456,
    "pending_ttl_ms": 60000,
    "event_total_bytes": 268435456,
    "preauth_per_peer_count": 64,
    "preauth_per_peer_bytes": 4194304,
    "preauth_total_count": 1024,
    "preauth_total_bytes": 67108864,
    "preauth_ttl_ms": 30000,
    "journal_max_entries": 65536,
    "journal_max_bytes": 536870912,
    "journal_obligation_ttl_ms": 86400000,
    "journal_terminal_retention_ms": 604800000,
    "journal_retry_interval_ms": 1000
  },
  "metric_peer_id": 9991000,
  "reconnect_base_delay_ms": 250,
  "reconnect_max_delay_ms": 30000
}
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "config\node-mainline-overlay.json.example")

  # Deployment JSON is not a transaction carrier. No fixture genesis, validator
  # identity, allocation or HTTP listener is invented by the packaging script.
  @'
{
  "profile": "native_execution_tick",
  "library": "/opt/novovm/aoem/linux/core/bin/libaoem_ffi.so",
  "database": "/var/lib/novovm/state/validator.rocksdb",
  "signing_key_file": "/var/lib/novovm/keys/validator.sealed",
  "transport_key_file": "/var/lib/novovm/keys/transport.sealed",
  "key_storage": "protected_v1",
  "udp_addr": "0.0.0.0:41000",
  "peers": [],
  "native_client_limit": 16,
  "genesis": null,
  "workers": 4,
  "execution_backend": "cpu_callbacks",
  "batch_size": 1024
}
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "config\native-node.json.example")

  @'
NOVOVM_NODE_MODE=native_execution_tick
NOVOVM_NATIVE_EXECUTION_CONFIG=/etc/novovm/native-node.json
# Normal service/restart must open the original database, never recreate it.
NOVOVM_NATIVE_EXECUTION_START=existing
NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS=0
NOVOVM_NATIVE_EXECUTION_TICK_INTERVAL_MS=1
NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH=/var/lib/novovm/reports/node-progress.json
NOVOVM_NATIVE_EXECUTION_PIPELINE_SUMMARY_REPORT_PATH=/var/lib/novovm/reports/node-summary.json
# The authenticated native owner supplies transport, signer, genesis and store.
# Do not enable PRODUCT_MAINLINE_OVERLAY, historical-http or a second owner.
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
WorkingDirectory=/var/lib/novovm
StateDirectory=novovm
ExecStartPre=/usr/bin/mkdir -p /var/lib/novovm/reports
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
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "compatibility\novovm-product-relay.service")

  # Native binary duplex relay is explicit. The positional legacy-json unit
  # above is retained only as a labelled compatibility resource, never default.
  @'
[Unit]
Description=NOVOVM NOVONET binary duplex relay
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=novovm
Group=novovm
WorkingDirectory=/var/lib/novovm-relay
StateDirectory=novovm-relay novovm-relay/reports
StateDirectoryMode=0700
ExecStart=/opt/novovm/bin/supervm-relay --runtime duplex-v2 /etc/novovm/relay.json
Restart=on-failure
RestartSec=5
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/novovm-relay

[Install]
WantedBy=multi-user.target
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "systemd\supervm-relay.service")

  Copy-Item -LiteralPath (Join-Path $repo 'config/rendezvous-linux/novonet-mailbox.service') `
    -Destination (Join-Path $packagePath 'systemd/novonet-mailbox.service')

  # Public configuration shapes only. They fail closed until the operator
  # supplies approved trust/listener information; no fake key or public host.
  @'
{
  "version": 2,
  "root": "/var/lib/novonet-mailbox",
  "allowed_clients": [],
  "relays": [],
  "nameserver": "REVIEW_LOCAL_DNS_ADDRESS",
  "bind_address": "0.0.0.0:43173",
  "relay_only": false,
  "record_id": "REVIEW_HOST_RECORD_ID"
}
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath 'config/mailbox-node.json.example')
  @'
{
  "version": 1,
  "listen": "0.0.0.0:43171",
  "advertised": "REVIEW_REACHABLE_HOST:43171",
  "max_connections": 16,
  "max_bytes_per_minute": 67108864
}
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath 'config/mailbox-native.json.example')
  @'
{
  "version": 1,
  "listen": "0.0.0.0:43172",
  "advertised": "REVIEW_REACHABLE_HOST:43172",
  "max_connections": 4,
  "native_mailbox_v8": true
}
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath 'config/mailbox-tls.json.example')

  @'
{
  "schema_version": 1,
  "host_id": "REVIEW_HOST_LABEL",
  "package_root": "/opt/novovm/REVIEW_FIXED_RELEASE",
  "expected_release_manifest_sha256": "REVIEW_RELEASE_MANIFEST_SHA256",
  "services": []
}
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath 'config/unified-node.json.example')

  @'
[Unit]
Description=NOVOVM Native NovoRUDP/APFL Validator
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=novovm
Group=novovm
EnvironmentFile=/etc/novovm/novovm-node.env
LoadCredentialEncrypted=owner-key:/etc/novovm/credentials/node-owner-key.cred
WorkingDirectory=/var/lib/novovm
StateDirectory=novovm novovm/state novovm/reports
StateDirectoryMode=0700
ExecStart=/opt/novovm/bin/novovm-node
Restart=always
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/novovm

[Install]
WantedBy=multi-user.target
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "systemd\novovm-node.service")

  foreach ($document in $packageDocs) {
    Copy-Item -LiteralPath (Join-Path $repo "docs/$document") -Destination (Join-Path $packagePath "docs")
  }

  @'
# NOVOVM / NOVONET Unified Node Linux Package

This is one headless bundle of the existing node, native client, communication
mailbox owner, relay and novovmctl management adapters. It contains no Rust
toolchain, IDE, Codex installation, genesis, private key or runtime database.
Packaging is not installation, startup, multi-host acceptance or mainnet launch.

1. Verify `CHECKSUMS.sha256` and the externally pinned release-manifest.json hash
   before deployment. build_contracts records Cargo crate, features and binary
   hashes. Keep each fixed release immutable; do not overwrite an active binary.
2. Review config/unified-node.json.example for:
   novovmctl lifecycle --deployment-manifest HOST.json --action status
   Its empty services and placeholders are intentionally unusable. Enumerate
   only reviewed role/manager/unit/binary/config pins and original protected data
   paths. Use --action plan for no service-manager calls; --apply is explicit.
   Default status performs read-only service observation. Install only
   registers reviewed platform units; it does not create accounts, credentials,
   authorize genesis, start services or manufacture economic capabilities.
3. Keep purpose-isolated identities, private key files, databases, signing
   journals and ciphertext under their ORIGINAL owner. Asset signing is
   ML-DSA-65; transport/consensus still have Ed25519 and are not full-PQ signoff.
4. systemd/novonet-mailbox.service is the existing self-owned owner unit with
   its ORIGINAL /opt/novonet-mailbox/current, /etc/novonet-mailbox, private data
   and systemd owner-key credential paths. resources/config/rendezvous-linux
   documents these requirements. Mailbox templates are intentionally incomplete;
   allowed_clients=[] does not mean open admission and missing state is not a
   request to create a replacement identity.
   resources/scripts/linux/install-owned-mailbox.sh is FIRST INSTALL ONLY; it
   provisions and starts that one service. The unified adapter does NOT invoke
   it. NEVER run it to adopt or upgrade B's existing owner. Existing deployments
   require exact existing unit/config/binary pins with existing=true. Admission,
   native-v8 carrier and protected policy are not silently changed by packaging.
   Current unified path validation rejects symlink/reparse ancestors, including
   B's /opt/novonet-mailbox/current alias. Do not rewrite B's unit or move its data
   to bypass this: reviewed alias-to-release adoption is still NOT IMPLEMENTED.
   systemd/supervm-relay.service explicitly selects --runtime duplex-v2. Its
   loopback example is not proof of public reachability. Existing B relay is not
   converted by this template: resources/config/relay preserves its original
   resources for reviewed adoption. compatibility/novovm-product-relay.service
   is historical positional legacy-json, NOT a unified default or fallback.
5. Read `docs/NOVOVM_MAINNET_OPERATOR_PRIMER.zh-CN.txt` before signing or genesis.
   Install a reviewed `native-node.json` and `novovm-node.env`. The config example
   is deliberately incomplete: replace `genesis: null` with the common verified
   GenesisConfig, pin its protocol/genesis commitments, populate every remote
   validator public key/address and its committed transport_public_key. Prepare
   TWO independent protected purpose keys for this node (validator and transport)
   using novovmctl native-identity under the eventual service account. Configure
   key_storage=protected_v1; raw_development is test tooling, not managed startup.
   The node unit requires its OWN encrypted systemd owner-key credential at
   /etc/novovm/credentials/node-owner-key.cred. Prepare it with the existing
   credential procedure under a node-specific owner; NEVER reuse mailbox/Relay
   credentials. Include that source directory in the node protected_paths.
   Before activation, run both native-identity inspect commands with the SAME
   service account and credential. Lifecycle plan validates paths and pins but
   reports custody_verified=false: it does not decrypt or generate private keys.
   See docs/NOVOVM_PURPOSE_IDENTITY_PREPARATION.md for the offline handoff.
   Do not use fixture keys, invent allocation values, or copy another node's DB.
   There is no HTTP/JSON-RPC listener. JSON here is deployment configuration only;
   native transaction/query traffic uses NovoRUDP/APFL.
6. Keep the bundled generic AOEM FULLMAX runtime under `/opt/novovm/aoem`;
   `novovm-node.env` pins its core, manifest, profile, and sidecar paths.
7. Normal service startup uses `NOVOVM_NATIVE_EXECUTION_START=existing`. Only
   after an operator approves a new empty test chain, load the reviewed env and
   run the node once in the foreground with START=create. Stop it normally
   before enabling the existing-mode service. Retain the original database and
   signing journal across every restart; create is not a recovery command.
8. Product relay/peer/topology tools and `node-mainline-overlay.json.example` are retained
   for their explicit communication/compatibility callers. They are NOT the
   native validator owner and must not be enabled beside native CONFIG. Their
   preflight/evidence does not qualify native finality or automatic NAT routing.
9. Check actual native query responses, AOEM-backed ledger readability, peer and
   recovery state, mailbox signed readiness and real communication separately.
   PID or listener status alone is not healthy delivery. A mailbox health check
   does not fabricate traffic, consume ciphertext or settle funds. Missing
   configuration is NOT_CONFIGURED; service settlement and general content
   replication remain unavailable. Generate signed post-run evidence with the
   existing `novovm-product-evidence` after actual acceptance.
10. Record source commit, executable and actual loaded AOEM hashes on each host.
    This package's checksums are not production, multi-host TPS, GPU or ZK signoff.
    Do not reuse a different DLL/SO's benchmark as this bundle's qualification.
    Rolling mixed-version operation is not claimed by this package.
    Windows SCM/task apply is NOT IMPLEMENTED by this Linux management adapter.
    Unified upgrade/rollback remain fail-closed until data/protocol compatibility
    is established; no --force override, clearing state or rollback of journals.
    An old package retained by -Force is not a safe service/data rollback by itself.

New installation and adoption are separate operations. Full installation does
not enroll validators or give equal workloads to all machines; the original
four-validator set still needs 3/4. Content custody/TTL is not generic replicated
storage. Funded service obligations are not rewards for being online; the test
NOV custody/retrieval -> specific UCA payout -> respend integration is NOT enabled.
This package must not change B's mailbox/relay/phone identities or C's App work.

The packager accepts only named directories under artifacts/product-overlay.
It stages validated artifacts first; -Force renames the old package to a unique
.previous directory instead of deleting it. Failed staging directories and old
packages remain recoverable and require separate intentional housekeeping.

The native validator uses its authenticated NovoRUDP owner; the communication
owner reuses native UDP/V8TLS and supported carriers. Neither TLS nor a CA is
the NOVOVM asset authority. No HTTP/JSON-RPC transaction listener is installed.

AOEM is a bundled third-party, domain-neutral execution engine. NOVOVM
authentication, transaction semantics, overlay routing, and product policy
remain in the host; no NOVOVM-specific business logic is added to AOEM.
'@ | Set-Content -NoNewline -Encoding ascii (Join-Path $packagePath "README.md")

  # Windows source checkouts may have CRLF. Linux env/unit files must not carry
  # a trailing CR into configuration values or a manually sourced START mode.
  foreach ($template in @(
      @(Get-ChildItem -LiteralPath (Join-Path $packagePath 'config') -File -Recurse) +
      @(Get-ChildItem -LiteralPath (Join-Path $packagePath 'systemd') -File -Recurse) +
      @(Get-ChildItem -LiteralPath (Join-Path $packagePath 'resources') -File -Recurse) +
      @(Get-ChildItem -LiteralPath (Join-Path $packagePath 'compatibility') -File -Recurse) +
      @(Get-Item -LiteralPath (Join-Path $packagePath 'README.md')))) {
    $content = [IO.File]::ReadAllText($template.FullName).Replace("`r`n", "`n")
    [IO.File]::WriteAllText($template.FullName, $content, [Text.UTF8Encoding]::new($false))
  }
  $entries = Get-ChildItem -LiteralPath $packagePath -File -Recurse |
    Where-Object { $_.Name -ne "CHECKSUMS.sha256" -and $_.Name -ne "release-manifest.json" } |
    ForEach-Object {
      $relative = $_.FullName.Substring($packageRoot.Length).TrimStart([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar).Replace([IO.Path]::DirectorySeparatorChar, [char]'/')
      $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
      [PSCustomObject]@{ path = $relative; sha256 = $hash; bytes = $_.Length }
    } | Sort-Object path
  $checksumLines = @($entries | ForEach-Object { "$($_.sha256)  $($_.path)" })
  [IO.File]::WriteAllText((Join-Path $packagePath "CHECKSUMS.sha256"),
    [string]::Join("`n", $checksumLines) + "`n", [Text.UTF8Encoding]::new($false))
  $endCommit = (git rev-parse HEAD).Trim()
  if ($LASTEXITCODE -ne 0 -or $endCommit -ne $commit) {
    throw "Source commit changed while packaging; existing package remains untouched."
  }
  $dirty = @(git status --porcelain=v1 --untracked-files=all)
  if ($LASTEXITCODE -ne 0 -or $dirty.Count -ne 0) {
    throw "Source worktree changed while packaging; existing package remains untouched."
  }
  $copiedAoem = Join-Path $packagePath "aoem/linux/core/bin/libaoem_ffi.so"
  if ((Get-FileHash -LiteralPath $copiedAoem -Algorithm SHA256).Hash.ToLowerInvariant() -ne $actualAoemSha) {
    throw "Copied AOEM runtime differs from its validated source; existing package remains untouched."
  }
  $ackWireVersion = "novovm-product-mainline-recipient-ack/v1"
  $journalSchema = "novovm-product-delivery-journal/v1"
  $overlaySource = Get-Content -Raw -LiteralPath (Join-Path $repo "crates\novovm-node\src\product_mainline_overlay.rs")
  $journalSource = Get-Content -Raw -LiteralPath (Join-Path $repo "crates\novovm-node\src\product_delivery_journal.rs")
  if (-not $overlaySource.Contains($ackWireVersion)) {
    throw "Release manifest ACK wire version does not match the Product Overlay source."
  }
  if (-not $journalSource.Contains($journalSchema)) {
    throw "Release manifest journal schema does not match the delivery-journal source."
  }
  [PSCustomObject]@{
    scope = "novovm_unified_linux_release_v3"
    target = $Target
    git_commit = $commit
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    node_entry = "native_execution_tick/NOVOVM_NATIVE_EXECUTION_CONFIG"
    native_release_http_listener = $false
    build_contracts = $buildContracts
    deployment = [PSCustomObject]@{
      schema_version = 1
      manifest_template = 'config/unified-node.json.example'
      manager = 'novovmctl lifecycle --deployment-manifest'
      services_configured = $false
      auto_start = $false
      new_chain_created = $false
      existing_services_modified = $false
      cross_version_upgrade_qualified = $false
      mailbox_owner_provisioned = $false
    }
    capabilities = [PSCustomObject]@{
      native_node_and_uca_spend = 'included_requires_approved_configuration_and_runtime_acceptance'
      mailbox_encrypted_object_custody = 'included_requires_existing_or_explicitly_provisioned_owner'
      relay_duplex_v2 = 'included_explicit_selection_no_legacy_fallback'
      full_stack_runtime_acceptance = 'not_performed_by_packaging'
      service_settlement_to_provider_uca = 'unavailable'
      general_content_replication_and_repair = 'unavailable'
      full_transport_and_consensus_mldsa = 'unavailable'
      business_proof_finality = 'unavailable'
    }
    aoem_runtime = [PSCustomObject]@{
      path = "aoem/linux/core/bin/libaoem_ffi.so"
      sha256 = $actualAoemSha
      platform = "linux-x86_64"
      performance_qualified = $false
    }
    compatibility_tools = [PSCustomObject]@{
      active_validator_owner = $false
      product_overlay_ack_wire = $ackWireVersion
      delivery_journal_schema = $journalSchema
    }
    mixed_version_rolling_upgrade_supported = $false
    signed_evidence_included = $false
    note = "Generate signed runtime evidence after deployment; this build manifest is checksum-only."
    artifacts = $entries
  } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $packagePath "release-manifest.json")
  Publish-LinuxPackage $repo $packagePath $destination -Replace:$Force
  Write-Host "Unified Linux artifacts assembled, not installed or started: $destination"
}
finally {
  Pop-Location
}
