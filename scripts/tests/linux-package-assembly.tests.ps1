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
foreach ($name in @('metadataFiles', 'packageDocs')) {
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
$fixtureState = @{commit='1234567890123456789012345678901234567890';root=$fixture;buildCalls=0}
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
  $fixtureState.buildCalls++
  $index = [Array]::IndexOf($args, '--bin')
  if ($index -lt 0) { throw "Unexpected cargo call: $args" }
  $name = $args[$index + 1]
  $artifact = Join-Path $fixtureState.root "build/$name"
  $alias = $artifact + '.deps'
  [IO.File]::WriteAllBytes($alias, [byte[]]@(0x7f, 0x45, 0x4c, 0x46))
  if (!(Test-Path -LiteralPath $artifact)) {
    New-Item -ItemType HardLink -Path $artifact -Target $alias | Out-Null
  }
  if ($fixtureState.tamper -and $fixtureState.buildCalls -eq 8) {
    # Another write through Cargo's deps alias after the first hash was pinned.
    [IO.File]::WriteAllBytes((Join-Path $fixtureState.root 'build/novovm-node.deps'), [byte[]]@(1, 2, 3, 4))
  }
  @{reason='compiler-artifact';target=@{name=$name;kind=@('bin')};executable=$artifact} | ConvertTo-Json -Compress
}
& $packager
$output = Join-Path $fixture 'artifacts/product-overlay/linux-x86_64'
$manifest = Get-Content -LiteralPath (Join-Path $output 'release-manifest.json') -Raw | ConvertFrom-Json
if ($manifest.git_commit -ne $fixtureState.commit -or $fixtureState.buildCalls -ne 8 -or
    $manifest.aoem_runtime.sha256 -ne $coreSha -or $manifest.native_release_http_listener -ne $false) {
  throw 'Package manifest, build count or native boundary differs from inputs'
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
foreach ($file in @(Get-ChildItem (Join-Path $output 'config') -File) + @(Get-ChildItem (Join-Path $output 'systemd') -File)) {
  if ([IO.File]::ReadAllText($file.FullName).Contains("`r")) { throw 'Linux template contains CR' }
}
if (!(Test-Path (Join-Path $output 'bin/novovm-native-client'))) { throw 'Native client missing' }
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
Write-Output "PASS: original full assembly, manifest, checksums, Linux templates and changed-hardlink rejection. Synthetic artifacts only; retained: $fixture"
