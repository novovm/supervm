# Run the ORIGINAL packager end to end with synthetic inputs and external-tool
# stubs. This tests assembly, not Linux compilation or executable qualification.
# Keep all generated evidence; never touch a real package, runtime or database.
$ErrorActionPreference = 'Stop'
$source = (Resolve-Path (Join-Path $PSScriptRoot '../novovm-package-product-linux.ps1')).Path
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$fixture = Join-Path $repo ('artifacts/audit/linux-package-assembly-' + [Guid]::NewGuid().ToString('N'))
foreach ($directory in @('scripts', 'docs', 'aoem/linux/core/bin', 'aoem/manifest', 'aoem/config', 'crates/novovm-node/src', 'build')) {
  New-Item -ItemType Directory -Path (Join-Path $fixture $directory) -Force | Out-Null
}
$packager = Join-Path $fixture 'scripts/novovm-package-product-linux.ps1'
Copy-Item -LiteralPath $source -Destination $packager
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw $parseErrors }
foreach ($name in @('metadataFiles', 'packageDocs', 'platformResources')) {
  $assignments = @($ast.FindAll({ param($node)
    $node -is [System.Management.Automation.Language.AssignmentStatementAst] -and
      $node.Left.Extent.Text -eq ('$' + $name)
  }, $true))
  if ($assignments.Count -ne 1) { throw "Expected one $name input list" }
  . ([scriptblock]::Create($assignments[0].Extent.Text))
}
foreach ($name in $metadataFiles) {
  [IO.File]::WriteAllText((Join-Path $fixture "aoem/$name"), 'synthetic metadata')
}
foreach ($name in $packageDocs) {
  [IO.File]::WriteAllText((Join-Path $fixture "docs/$name"), "synthetic document: $name")
}
foreach ($name in $platformResources) {
  $target = Join-Path $fixture $name
  New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($target)) -Force | Out-Null
  Copy-Item -LiteralPath (Join-Path $repo $name) -Destination $target
}
$core = Join-Path $fixture 'aoem/linux/core/bin/libaoem_ffi.so'
[IO.File]::WriteAllBytes($core, [byte[]]@(0x7f, 0x45, 0x4c, 0x46))
$coreSha = (Get-FileHash -LiteralPath $core -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path $fixture 'aoem/linux/manifest.json'), '{"profile":"fullmax","platform":"linux"}')
[IO.File]::WriteAllText((Join-Path $fixture 'aoem/aoem-sdk-manifest.json'),
  (@{platforms=@{'linux-x86_64'=@{library_sha256=$coreSha}}} | ConvertTo-Json -Depth 4))
foreach ($name in @('aoem/manifest/aoem-manifest.json', 'aoem/config/aoem-runtime-profile.json')) {
  [IO.File]::WriteAllText((Join-Path $fixture $name), '{}')
}
[IO.File]::WriteAllText((Join-Path $fixture 'crates/novovm-node/src/product_mainline_overlay.rs'), 'novovm-product-mainline-recipient-ack/v1')
[IO.File]::WriteAllText((Join-Path $fixture 'crates/novovm-node/src/product_delivery_journal.rs'), 'novovm-product-delivery-journal/v1')
$fixtureState = @{commit='1234567890123456789012345678901234567890';root=$fixture;buildCalls=0;contracts=@()}
function git {
  $global:LASTEXITCODE = 0
  if ($args[0] -eq 'status') { return }
  if ($args[0] -eq 'rev-parse' -and $args[1] -eq 'HEAD') { return $fixtureState.commit }
  throw "Unexpected git call: $args"
}
function rustup {
  $global:LASTEXITCODE = 0
  if (($args -join ' ') -ne 'target list --installed') { throw "Unexpected rustup call: $args" }
  'x86_64-unknown-linux-gnu'
}
function cargo {
  $global:LASTEXITCODE = 0
  if ($args[0] -eq 'metadata') {
    @{packages=@(@('novovm-node', 'novovm-relay', 'novovmctl') | ForEach-Object {
      @{name=$_;id="path+file://fixture#$_@0.1.0"}
    })} | ConvertTo-Json -Depth 4 -Compress
    return
  }
  $fixtureState.buildCalls++
  $index = [Array]::IndexOf($args, '--bin')
  if ($index -lt 0) { throw "Unexpected cargo call: $args" }
  $name = $args[$index + 1]
  $package = $args[[Array]::IndexOf($args, '-p') + 1]
  $featureIndex = [Array]::IndexOf($args, '--features')
  $features = if ($featureIndex -ge 0) { @($args[$featureIndex + 1].Split(',')) } else { @() }
  $fixtureState.contracts += @{package=$package;binary=$name;features=$features}
  $artifact = Join-Path $fixtureState.root "build/$name"
  $alias = $artifact + '.deps'
  [IO.File]::WriteAllBytes($alias, [byte[]]@(0x7f, 0x45, 0x4c, 0x46))
  if (!(Test-Path -LiteralPath $artifact)) {
    New-Item -ItemType HardLink -Path $artifact -Target $alias | Out-Null
  }
  if ($fixtureState.tamper -and $fixtureState.buildCalls -eq 11) {
    # Another write through Cargo's deps alias after the first hash was pinned.
    [IO.File]::WriteAllBytes((Join-Path $fixtureState.root 'build/novovm-node.deps'), [byte[]]@(1, 2, 3, 4))
  }
  @{reason='compiler-artifact';package_id="path+file://fixture#$package@0.1.0";features=$features;target=@{name=$name;kind=@('bin')};executable=$artifact} | ConvertTo-Json -Compress
}
& $packager
$output = Join-Path $fixture 'artifacts/product-overlay/linux-x86_64'
$manifest = Get-Content -LiteralPath (Join-Path $output 'release-manifest.json') -Raw | ConvertFrom-Json
if ($manifest.git_commit -ne $fixtureState.commit -or $fixtureState.buildCalls -ne 11 -or
    $manifest.aoem_runtime.sha256 -ne $coreSha -or $manifest.native_release_http_listener -ne $false) {
  throw 'Package manifest, build count or native boundary differs from inputs'
}
if ($manifest.scope -ne 'novovm_unified_linux_release_v3' -or $manifest.build_contracts.Count -ne 11 -or
    $manifest.deployment.auto_start -ne $false -or $manifest.deployment.new_chain_created -ne $false -or
    $manifest.deployment.existing_services_modified -ne $false -or $manifest.deployment.services_configured -ne $false -or
    $manifest.capabilities.service_settlement_to_provider_uca -ne 'unavailable' -or
    $manifest.capabilities.general_content_replication_and_repair -ne 'unavailable') {
  throw 'Unified manifest must record actual builds without claiming activation or unavailable capabilities'
}
foreach ($contract in $manifest.build_contracts) {
  $called = @($fixtureState.contracts | Where-Object binary -eq $contract.binary)
  if ($called.Count -ne 1 -or $called[0].package -ne $contract.package -or $contract.sha256 -ne $coreSha -or
      $contract.cargo_package_id -ne "path+file://fixture#$($contract.package)@0.1.0" -or
      ($called[0].features -join ',') -ne ($contract.cargo_features -join ',')) {
    throw "Build contract does not match actual Cargo invocation/artifact: $($contract.binary)"
  }
}
foreach ($bin in @('novovm-rendezvous-service', 'supervm-relay')) {
  $contract = @($manifest.build_contracts | Where-Object binary -eq $bin)[0]
  if ($contract.package -ne 'novovm-relay' -or $contract.features -notcontains 'iroh-transport' -or
      $contract.features -notcontains 'tls-transport') { throw "Communication feature contract missing: $bin" }
}
if (@($manifest.build_contracts | Where-Object binary -eq 'novovmctl')[0].package -ne 'novovmctl') {
  throw 'Existing management binary must be built from its original crate'
}
foreach ($entry in $manifest.artifacts) {
  if ($entry.path.Contains('\') -or [IO.Path]::IsPathRooted($entry.path)) { throw 'Nonportable checksum path' }
  $file = Join-Path $output $entry.path
  if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256 -or
      (Get-Item -LiteralPath $file).Length -ne $entry.bytes) { throw "Artifact mismatch: $file" }
}
$lines = @(Get-Content -LiteralPath (Join-Path $output 'CHECKSUMS.sha256'))
if ($lines.Count -ne $manifest.artifacts.Count) { throw 'Checksum inventory differs from manifest' }
foreach ($entry in $manifest.artifacts) {
  if ($lines -cnotcontains "$($entry.sha256)  $($entry.path)") { throw "Missing checksum: $($entry.path)" }
}
foreach ($file in @(Get-ChildItem (Join-Path $output 'config') -File -Recurse) +
    @(Get-ChildItem (Join-Path $output 'systemd') -File -Recurse) +
    @(Get-ChildItem (Join-Path $output 'resources') -File -Recurse) +
    @(Get-ChildItem (Join-Path $output 'compatibility') -File -Recurse)) {
  if ([IO.File]::ReadAllText($file.FullName).Contains("`r")) { throw 'Linux template contains CR' }
}
if (!(Test-Path (Join-Path $output 'bin/novovm-native-client'))) { throw 'Native client missing' }
$managerConfig = Get-Content -LiteralPath (Join-Path $output 'config/unified-node.json.example') -Raw | ConvertFrom-Json
if ($managerConfig.schema_version -ne 1 -or $managerConfig.services.Count -ne 0 -or
    $managerConfig.expected_release_manifest_sha256 -notlike 'REVIEW_*') {
  throw 'Deployment example must remain explicitly unconfigured, not fake credentials/services'
}
foreach ($resource in $platformResources) {
  $expected = [IO.File]::ReadAllText((Join-Path $repo $resource)).Replace("`r`n", "`n")
  if ([IO.File]::ReadAllText((Join-Path $output "resources/$resource")) -cne $expected) {
    throw "Original platform resource was altered: $resource"
  }
}
$relayUnit = Get-Content -LiteralPath (Join-Path $output 'systemd/supervm-relay.service') -Raw
if ($relayUnit -notmatch 'ExecStart=/opt/novovm/bin/supervm-relay --runtime duplex-v2 /etc/novovm/relay.json' -or
    (Test-Path (Join-Path $output 'systemd/novovm-product-relay.service')) -or
    !(Test-Path (Join-Path $output 'compatibility/novovm-product-relay.service'))) {
  throw 'Legacy positional relay must not be the unified default'
}
$mailboxUnit = Get-Content -LiteralPath (Join-Path $output 'systemd/novonet-mailbox.service') -Raw
if ($mailboxUnit -notmatch 'LoadCredentialEncrypted=owner-key:' -or $mailboxUnit -notmatch '/opt/novonet-mailbox/current/') {
  throw 'Original owner credential and installation requirements were lost'
}
foreach ($unit in Get-ChildItem -LiteralPath (Join-Path $output 'systemd') -File) {
  $text = Get-Content -LiteralPath $unit.FullName -Raw
  if ($text -match '(?m)^Exec(StartPre|StartPost|Stop|StopPost|Reload|Condition)=') {
    throw "Active unit contains an unsupported executable hook: $($unit.Name)"
  }
}
$nodeUnit = Get-Content -LiteralPath (Join-Path $output 'systemd/novovm-node.service') -Raw
if ($nodeUnit -notmatch '(?m)^LoadCredentialEncrypted=owner-key:/etc/novovm/credentials/node-owner-key\.cred$') {
  throw 'Node must bind its own encrypted owner credential, not borrow the mailbox owner'
}
$nativeConfig = Get-Content -LiteralPath (Join-Path $output 'config/native-node.json.example') -Raw | ConvertFrom-Json
if ($nativeConfig.key_storage -ne 'protected_v1' -or
    $nativeConfig.signing_key_file -eq $nativeConfig.transport_key_file -or
    $nativeConfig.transport_key_file -ne '/var/lib/novovm/keys/transport.sealed') {
  throw 'Native package must require independent protected validator and transport custody'
}
if ($nodeUnit -notmatch '(?m)^StateDirectory=novovm novovm/state novovm/reports$' -or
    $relayUnit -notmatch '(?m)^StateDirectory=novovm-relay novovm-relay/reports$') {
  throw 'Active units must declare their persistent directories without execution hooks'
}
$relayConfig = Get-Content -LiteralPath (Join-Path $output 'config/relay.json.example') -Raw | ConvertFrom-Json
if ($relayUnit -notmatch '(?m)^WorkingDirectory=/var/lib/novovm-relay$' -or
    $relayUnit -notmatch '(?m)^ReadWritePaths=/var/lib/novovm-relay$' -or
    $relayConfig.report_path -ne '/var/lib/novovm-relay/reports/relay.json') {
  throw 'Fresh relay protected data must not overlap node or mailbox owner data'
}
$fixtureState.buildCalls = 0
$fixtureState.tamper = $true
$rejected = $false
try { & $packager -PackageDir 'artifacts/product-overlay/changed-artifact' } catch {
  if ($_.Exception.Message -notlike '*Cargo executable changed before/during copying*') { throw }
  $rejected = $true
}
if (!$rejected -or (Test-Path (Join-Path $fixture 'artifacts/product-overlay/changed-artifact'))) {
  throw 'Changed Cargo artifact must fail before package publication'
}
if ((Get-FileHash (Join-Path $output 'bin/novovm-node') -Algorithm SHA256).Hash.ToLowerInvariant() -ne $coreSha) {
  throw 'Package copy still aliases mutable Cargo source'
}
Write-Output "PASS: original unified assembly (11 binary contracts), manifest, checksums, original resources, unconfigured management, explicit duplex default and changed-hardlink rejection. Synthetic artifacts only; retained: $fixture"
