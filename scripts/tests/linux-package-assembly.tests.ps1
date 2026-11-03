# Run the ORIGINAL packager end to end with synthetic inputs and external-tool
# stubs. This tests assembly, not Linux compilation or executable qualification.
# Keep all generated evidence; never touch a real package, runtime or database.
param([ValidateSet('x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc')][string]$Target = 'x86_64-unknown-linux-gnu')
$ErrorActionPreference = 'Stop'
$windowsPackage = $Target -eq 'x86_64-pc-windows-msvc'
$platform = if ($windowsPackage) { 'windows' } else { 'linux' }
$platformId = "$platform-x86_64"
$coreName = if ($windowsPackage) { 'aoem_ffi.dll' } else { 'libaoem_ffi.so' }
$binarySuffix = if ($windowsPackage) { '.exe' } else { '' }
$source = (Resolve-Path (Join-Path $PSScriptRoot '../novovm-package-product-linux.ps1')).Path
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$fixture = Join-Path $repo ('artifacts/audit/linux-package-assembly-' + [Guid]::NewGuid().ToString('N'))
foreach ($directory in @('scripts', 'docs', "aoem/$platform/core/bin", 'aoem/manifest', 'aoem/config', 'crates/novovm-node/src', 'build')) {
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
  $resourceTarget = Join-Path $fixture $name
  New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($resourceTarget)) -Force | Out-Null
  Copy-Item -LiteralPath (Join-Path $repo $name) -Destination $resourceTarget
}
$core = Join-Path $fixture "aoem/$platform/core/bin/$coreName"
$artifactBytes = if ($windowsPackage) {
  $peBytes = [byte[]]::new(90)
  $peBytes[0]=0x4d; $peBytes[1]=0x5a; $peBytes[60]=64
  $peBytes[64]=0x50; $peBytes[65]=0x45; $peBytes[68]=0x64; $peBytes[69]=0x86
  $peBytes[88]=0x0b; $peBytes[89]=0x02
  $peBytes
} else {
  $elfBytes = [byte[]]::new(20)
  $elfBytes[0]=0x7f; $elfBytes[1]=0x45; $elfBytes[2]=0x4c; $elfBytes[3]=0x46
  $elfBytes[4]=2; $elfBytes[5]=1; $elfBytes[18]=62
  $elfBytes
}
[IO.File]::WriteAllBytes($core, $artifactBytes)
$coreSha = (Get-FileHash -LiteralPath $core -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path $fixture "aoem/$platform/manifest.json"), (@{profile='fullmax';platform=$platform} | ConvertTo-Json))
[IO.File]::WriteAllText((Join-Path $fixture 'aoem/aoem-sdk-manifest.json'),
  (@{platforms=@{$platformId=@{library_sha256=$coreSha}}} | ConvertTo-Json -Depth 4))
foreach ($name in @('aoem/manifest/aoem-manifest.json', 'aoem/config/aoem-runtime-profile.json')) {
  [IO.File]::WriteAllText((Join-Path $fixture $name), '{}')
}
[IO.File]::WriteAllText((Join-Path $fixture 'crates/novovm-node/src/product_mainline_overlay.rs'), 'novovm-product-mainline-recipient-ack/v2')
[IO.File]::WriteAllText((Join-Path $fixture 'crates/novovm-node/src/product_delivery_journal.rs'), 'novovm-product-delivery-journal/v1')
$fixtureState = @{commit='1234567890123456789012345678901234567890';root=$fixture;buildCalls=0;contracts=@();qualificationCalls=0;qualificationMode='valid'}
function git {
  $global:LASTEXITCODE = 0
  if ($args[0] -eq 'status') { return }
  if ($args[0] -eq 'rev-parse' -and $args[1] -eq 'HEAD') { return $fixtureState.commit }
  throw "Unexpected git call: $args"
}
function rustup {
  $global:LASTEXITCODE = 0
  if (($args -join ' ') -ne 'target list --installed') { throw "Unexpected rustup call: $args" }
  $Target
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
  $artifact = Join-Path $fixtureState.root "build/$name$binarySuffix"
  $alias = $artifact + '.deps'
  [IO.File]::WriteAllBytes($alias, $artifactBytes)
  if (!(Test-Path -LiteralPath $artifact)) {
    New-Item -ItemType HardLink -Path $artifact -Target $alias | Out-Null
  }
  if ($fixtureState.tamper -and $fixtureState.buildCalls -eq 11) {
    # Another write through Cargo's deps alias after the first hash was pinned.
    [IO.File]::WriteAllBytes((Join-Path $fixtureState.root "build/novovm-node$binarySuffix.deps"), [byte[]]@(1, 2, 3, 4))
  }
  @{reason='compiler-artifact';package_id="path+file://fixture#$package@0.1.0";features=$features;target=@{name=$name;kind=@('bin')};executable=$artifact} | ConvertTo-Json -Compress
}
# Only this synthetic command path is stubbed. The production packager still
# invokes its exact Cargo-reported ctl, with no skip/injection option.
$runtimeFixtureCommand = Join-Path $fixture "build/novovmctl$binarySuffix"
Set-Item -LiteralPath ('Function:\global:' + $runtimeFixtureCommand) -Value {
  param([Parameter(ValueFromRemainingArguments)]$RuntimeArguments)
  $fixtureState.qualificationCalls++
  if ($RuntimeArguments.Count -ne 6 -or $RuntimeArguments[0] -cne 'native-identity' -or
      $RuntimeArguments[1] -cne 'qualify-runtime' -or $RuntimeArguments[2] -cne '--library' -or
      $RuntimeArguments[3] -ne $core -or
      $RuntimeArguments[4] -cne '--library-sha256' -or $RuntimeArguments[5] -cne $coreSha) {
    throw 'Original runtime preflight must consume the exact built ctl, selected core and independent pin'
  }
  $global:LASTEXITCODE = 0
  if ($fixtureState.qualificationMode -eq 'nonzero') {
    $global:LASTEXITCODE = 17
    return '{}'
  }
  $report = @{
    ok=$true; command='native-identity'; data=@{
      action='qualify-runtime'; qualification='ml-dsa-65-native-verification-owner'; algorithm='ML-DSA-65';
      signature_suite=65; library=$RuntimeArguments[3]; library_sha256=$RuntimeArguments[5];
      verification_qualified=$true; scalar_verification_kat_passed=$true;
      unified_semantic_verification_kat_passed=$true; mixed_batch_verification_kat_passed=$true;
      primitive_sign_interface_size_checked=$true; signing_tested=$false; keygen_tested=$false;
      protected_key_created=$false; custody_accessed=$false; database_created=$false;
      service_started=$false; whole_node_post_quantum_qualified=$false
    }
  }
  switch ($fixtureState.qualificationMode) {
    'missing' { $report.Remove('data') }
    'wrong-hash' { $report.data.library_sha256 = '0' * 64 }
    'wrong-scope' { $report.data.qualification = 'capability-bit-only' }
    'missing-batch' { $report.data.Remove('mixed_batch_verification_kat_passed') }
    'false-batch' { $report.data.mixed_batch_verification_kat_passed = $false }
  }
  $report | ConvertTo-Json -Depth 4 -Compress
}
& $packager -Target $Target
$output = Join-Path $fixture "artifacts/product-overlay/$platform-x86_64"
$manifest = Get-Content -LiteralPath (Join-Path $output 'release-manifest.json') -Raw | ConvertFrom-Json
if ($manifest.git_commit -ne $fixtureState.commit -or $fixtureState.buildCalls -ne 11 -or
    $manifest.aoem_runtime.sha256 -ne $coreSha -or $manifest.native_release_http_listener -ne $false) {
  throw 'Package manifest, build count or native boundary differs from inputs'
}
if ($fixtureState.qualificationCalls -ne 1 -or !$manifest.aoem_runtime.verification_qualified -or
    $manifest.aoem_runtime.verification_qualification_scope -ne 'ml-dsa-65-native-verification-owner' -or
    $manifest.aoem_runtime.qualification_report -ne 'reports/native-identity-runtime-qualification.json' -or
    $manifest.aoem_runtime.signing_tested_by_preflight -ne $false -or
    $manifest.aoem_runtime.keygen_tested_by_preflight -ne $false -or
    $manifest.aoem_runtime.performance_qualified -ne $false) {
  throw 'Mock preflight call/report must be recorded with its verification-only scope, never runtime/signing/TPS acceptance'
}
$qualification = Get-Content -LiteralPath (Join-Path $output $manifest.aoem_runtime.qualification_report) -Raw | ConvertFrom-Json
if ($qualification.data.library_sha256 -cne $coreSha -or !$qualification.data.mixed_batch_verification_kat_passed) {
  throw 'The original runtime qualification output must be retained for the exact selected core'
}
$expectedScope = if ($windowsPackage) { 'novovm_unified_windows_release_v1' } else { 'novovm_unified_linux_release_v3' }
if ($manifest.scope -ne $expectedScope -or $manifest.target -ne $Target -or $manifest.build_contracts.Count -ne 11 -or
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
if (!(Test-Path (Join-Path $output "bin/novovm-native-client$binarySuffix"))) { throw 'Native client missing' }
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
if (!$windowsPackage) {
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
$packageReadme = Get-Content -LiteralPath (Join-Path $output 'README.md') -Raw
if ($packageReadme -notmatch 'ML-DSA-65 validator public key \(1952 bytes\)' -or
    $packageReadme -notmatch 'transport_public_key \(1952 bytes\)' -or
    $packageReadme -notmatch 'with NVPK0003 envelopes' -or
    $packageReadme -notmatch '\-\-library-sha256' -or
    $packageReadme -match 'transport/consensus still have Ed25519') {
  throw 'Unified package must document the current ML-DSA-65 validator contract and refuse old custody'
}
}
$expectedLibrary = if ($windowsPackage) { 'C:/ProgramData/NOVOVM/releases/REVIEW_FIXED_RELEASE/aoem/windows/core/bin/aoem_ffi.dll' } else { '/opt/novovm/aoem/linux/core/bin/libaoem_ffi.so' }
foreach ($name in @('relay', 'peer-sender', 'peer-receiver', 'node-mainline-overlay', 'native-nat', 'mailbox-node')) {
  $example = Get-Content -LiteralPath (Join-Path $output "config/$name.json.example") -Raw | ConvertFrom-Json
  if ($example.chain_id -ne 'REVIEW_APPROVED_CHAIN_ID' -or
      $example.library -ne $expectedLibrary -or
      $example.library_sha256 -cne $coreSha) {
    throw "Transport example must pin the actual bundled core and require approved chain configuration: $name"
  }
  $keyPath = switch ($name) {
    'relay' { $example.relay_identity_key_path }
    'mailbox-node' { $example.transport_identity_path }
    default { $example.identity_key_path }
  }
  if ($keyPath -notlike '*.sealed') { throw "Protected transport purpose key required: $name" }
}
$overlayExample = Get-Content -LiteralPath (Join-Path $output 'config/node-mainline-overlay.json.example') -Raw | ConvertFrom-Json
$natExample = Get-Content -LiteralPath (Join-Path $output 'config/native-nat.json.example') -Raw | ConvertFrom-Json
$mailboxExample = Get-Content -LiteralPath (Join-Path $output 'config/mailbox-node.json.example') -Raw | ConvertFrom-Json
if ($overlayExample.overlay.trusted_signer_public_keys[0] -ne 'REVIEW_REPLACE_WITH_FULL_1952_BYTE_MLDSA65_PUBLIC_KEY_ARRAY' -or
    $natExample.version -ne 2 -or $natExample.peer_public_key -ne 'REVIEW_REPLACE_WITH_FULL_1952_BYTE_MLDSA65_PUBLIC_KEY_ARRAY' -or
    $mailboxExample.version -ne 3 -or $mailboxExample.allowed_clients.Count -ne 0 -or
    $manifest.compatibility_tools.product_overlay_ack_wire -ne 'novovm-product-mainline-recipient-ack/v2') {
  throw 'Current identity/ACK contracts require explicit full-width public pins, not a fake 32-byte fixture or legacy config'
}
foreach ($name in @('relay', 'peer-sender', 'peer-receiver', 'node-mainline-overlay', 'native-nat', 'topology-plan')) {
  $text = Get-Content -LiteralPath (Join-Path $output "config/$name.json.example") -Raw
  if ($text -match 'ed25519|APPROVED_AOEM_CORE_SHA256') { throw "Old identity or unsubstituted library pin in $name" }
}
if ($natExample.expected_peer_id -notlike 'novovm-mldsa65:*' -or
    @($overlayExample.peers | Where-Object peer_id -notlike 'novovm-mldsa65:*').Count -ne 0) {
  throw 'Peer IDs must name the canonical ML-DSA-65 digest representation, not use the digest as a public key'
}
if (!$windowsPackage) {
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
} else {
  if (@(Get-ChildItem -LiteralPath (Join-Path $output 'systemd') -File).Count -ne 0 -or
      $manifest.aoem_runtime.platform -ne 'windows-x86_64' -or
      $manifest.aoem_runtime.path -ne 'aoem/windows/core/bin/aoem_ffi.dll') {
    throw 'Windows candidate must contain its own runtime and no Linux startup units'
  }
  foreach ($entry in $manifest.build_contracts) {
    if (!(Test-Path (Join-Path $output "bin/$($entry.binary).exe"))) { throw 'Windows Cargo executable extension lost' }
  }
  $nativeConfig = Get-Content -LiteralPath (Join-Path $output 'config/native-node.json.example') -Raw | ConvertFrom-Json
  if ($nativeConfig.library -ne $expectedLibrary -or $nativeConfig.key_storage -ne 'protected_v1' -or
      $nativeConfig.signing_key_file -eq $nativeConfig.transport_key_file -or
      $managerConfig.package_root -notlike 'C:/ProgramData/NOVOVM/releases/REVIEW_FIXED_RELEASE*') {
    throw 'Windows templates must retain protected purpose separation and explicit release paths'
  }
  $windowsReadme = Get-Content -LiteralPath (Join-Path $output 'README.md') -Raw
  if ($windowsReadme -notmatch 'NVPK0003' -or $windowsReadme -notmatch 'does not execute them' -or
      $windowsReadme -notmatch 'SCM/task acceptance is separate') { throw 'Windows packaging must not claim installation/activation' }
}
$fixtureState.buildCalls = 0
$fixtureState.tamper = $true
$rejected = $false
  try { & $packager -Target $Target -PackageDir 'artifacts/product-overlay/changed-artifact' } catch {
  if ($_.Exception.Message -notlike '*Cargo executable changed before/during copying*') { throw }
  $rejected = $true
}
if (!$rejected -or (Test-Path (Join-Path $fixture 'artifacts/product-overlay/changed-artifact'))) {
  throw 'Changed Cargo artifact must fail before package publication'
}
if ((Get-FileHash (Join-Path $output "bin/novovm-node$binarySuffix") -Algorithm SHA256).Hash.ToLowerInvariant() -ne $coreSha) {
  throw 'Package copy still aliases mutable Cargo source'
}
$fixtureState.tamper = $false
foreach ($mode in @('nonzero', 'missing', 'wrong-hash', 'wrong-scope', 'missing-batch', 'false-batch')) {
  $fixtureState.qualificationMode = $mode
  $callsBefore = $fixtureState.qualificationCalls
  $originalManifestSha = (Get-FileHash -LiteralPath (Join-Path $output 'release-manifest.json') -Algorithm SHA256).Hash
  $rejected = $false
  try { & $packager -Target $Target -PackageDir "artifacts/product-overlay/rejected-qualification-$mode" } catch {
    if ($_.Exception.Message -notlike '*Native identity runtime qualification*') { throw }
    $rejected = $true
  }
  if (!$rejected -or $fixtureState.qualificationCalls -ne ($callsBefore + 1) -or
      (Test-Path (Join-Path $fixture "artifacts/product-overlay/rejected-qualification-$mode")) -or
      (Get-FileHash -LiteralPath (Join-Path $output 'release-manifest.json') -Algorithm SHA256).Hash -ne $originalManifestSha) {
    throw "Runtime qualification rejection must stop publication and preserve the prior package: $mode"
  }
}
$fixtureState.qualificationMode = 'valid'
foreach ($case in @(
    @{root=(Join-Path $fixture 'aoem'); pin=''; message='*requires an independently pinned*'},
    @{root=''; pin=$coreSha; message='*requires AoemPackageRoot*'},
    @{root=(Join-Path $fixture 'aoem'); pin=('0' * 64); message='*does not match the independently pinned*'}
)) {
  $rejected = $false
  try {
    & $packager -Target $Target -PackageDir 'artifacts/product-overlay/rejected-runtime' -AoemPackageRoot $case.root -AoemCoreSha256 $case.pin
  } catch {
    if ($_.Exception.Message -notlike $case.message) { throw }
    $rejected = $true
  }
  if (!$rejected -or (Test-Path (Join-Path $fixture 'artifacts/product-overlay/rejected-runtime'))) {
    throw 'An isolated runtime must require a matching independent pin before publishing'
  }
}
& $packager -Target $Target -PackageDir 'artifacts/product-overlay/isolated-runtime' -AoemPackageRoot (Join-Path $fixture 'aoem') -AoemCoreSha256 $coreSha
$isolated = Get-Content -Raw -LiteralPath (Join-Path $fixture 'artifacts/product-overlay/isolated-runtime/release-manifest.json') | ConvertFrom-Json
if (!$isolated.aoem_runtime.isolated_package_input -or $isolated.aoem_runtime.sha256 -ne $coreSha) {
  throw 'The original assembly must bind and record its explicit isolated AOEM input'
}
Write-Output "PASS: original unified assembly (11 binary contracts), verification-only runtime preflight plus 6 failure propagation cases, manifest, checksums, original resources, unconfigured management, explicit duplex default and changed-hardlink rejection. Synthetic artifacts only; retained: $fixture"
