# No builds, Linux execution or deletion. Only generated fixture directories are
# renamed; all contents remain available for inspection under artifacts/audit.
$ErrorActionPreference = 'Stop'
$path = Join-Path $PSScriptRoot '../novovm-package-product-linux.ps1'
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
  (Resolve-Path $path).Path, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw $parseErrors }
foreach ($helper in $ast.FindAll({ param($node)
  $node -is [System.Management.Automation.Language.FunctionDefinitionAst]
}, $true)) { . ([scriptblock]::Create($helper.Extent.Text)) }
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$fixture = Join-Path $repo ('artifacts/audit/linux-package-safety-' + [Guid]::NewGuid().ToString('N'))
$outputRoot = Join-Path $fixture 'artifacts/product-overlay'
New-Item -ItemType Directory -Path $outputRoot | Out-Null
$checks = 0
function Expect-Rejection([scriptblock]$Action, [string]$Message) {
  $rejected = $false
  try { & $Action | Out-Null } catch {
    if ($_.Exception.Message -notlike $Message) { throw }
    $rejected = $true
  }
  if (!$rejected) { throw "Expected rejection: $Message" }
  $script:checks++
}
foreach ($directory in @('.', '..', '../AOEM', 'artifacts', 'artifacts/product-overlay',
    'artifacts/product-overlay/../../source', 'artifacts/product-overlay/package/child',
    'artifacts/product-overlay/a:stream', 'artifacts/product-overlay/invalid*name', $repo)) {
  Expect-Rejection { Resolve-PackageDestination $fixture $directory } '*PackageDir must name one package*'
}
Expect-Rejection { Resolve-PackageDestination $fixture '' } '*cannot be empty*'
$destination = Join-Path $outputRoot 'linux-x86_64'
if ((Resolve-PackageDestination $fixture 'artifacts/product-overlay/./linux-x86_64') -ne $destination) {
  throw 'Valid normalized named output rejected'
}
$checks++
New-Item -ItemType Directory -Path $destination | Out-Null
[IO.File]::WriteAllText((Join-Path $destination 'original.txt'), 'original retained')
$staged = Join-Path $outputRoot 'linux-x86_64.staging.fixture'
New-Item -ItemType Directory -Path $staged | Out-Null
[IO.File]::WriteAllText((Join-Path $staged 'new.txt'), 'new staged package')
Expect-Rejection { Publish-LinuxPackage $fixture $staged $destination } '*already exists*'
if (!(Test-Path -LiteralPath (Join-Path $destination 'original.txt')) -or !(Test-Path -LiteralPath (Join-Path $staged 'new.txt'))) {
  throw 'Refused replacement changed old or staged files'
}
$checks++
Expect-Rejection { Publish-LinuxPackage $fixture $destination $destination -Replace } '*distinct completed staging*'
Publish-LinuxPackage $fixture $staged $destination -Replace
$previous = @(Get-ChildItem -LiteralPath $outputRoot -Directory | Where-Object Name -like 'linux-x86_64.previous.*')
if ($previous.Count -ne 1 -or
    [IO.File]::ReadAllText((Join-Path $previous[0].FullName 'original.txt')) -ne 'original retained' -or
    [IO.File]::ReadAllText((Join-Path $destination 'new.txt')) -ne 'new staged package') {
  throw 'Recoverable publication did not retain original and new package contents'
}
$checks++
Expect-Rejection { Publish-LinuxPackage $fixture (Join-Path $outputRoot 'missing') $destination -Replace } '*distinct completed staging*'

# Real directory redirection fixture, not just string pattern matching. Never
# traverse or remove it: on Windows this is a junction; on Unix, a symlink.
$linkTarget = Join-Path $fixture 'external-fixture'
New-Item -ItemType Directory -Path $linkTarget | Out-Null
[IO.File]::WriteAllText((Join-Path $linkTarget 'protected.txt'), 'outside package')
$linkType = if ([IO.Path]::DirectorySeparatorChar -eq '\') { 'Junction' } else { 'SymbolicLink' }
$link = Join-Path $outputRoot 'redirect'
New-Item -ItemType $linkType -Path $link -Target $linkTarget | Out-Null
Expect-Rejection { Resolve-PackageDestination $fixture $link } '*links/reparse points*'
$nested = Join-Path $destination 'nested-link'
New-Item -ItemType $linkType -Path $nested -Target $linkTarget | Out-Null
Expect-Rejection { Resolve-PackageDestination $fixture $destination } '*links/reparse points*'
$redirectedRepository = Join-Path $fixture 'redirected-repository'
New-Item -ItemType Directory -Path $redirectedRepository | Out-Null
New-Item -ItemType $linkType -Path (Join-Path $redirectedRepository 'artifacts') -Target $linkTarget | Out-Null
Expect-Rejection { Resolve-PackageDestination $redirectedRepository 'artifacts/product-overlay/package' } '*links/reparse points*'
if ([IO.File]::ReadAllText((Join-Path $linkTarget 'protected.txt')) -ne 'outside package') {
  throw 'Link target content was changed'
}
$checks++

# Check the actual emitted template AST, without running the packager.
$strings = @($ast.FindAll({ param($node)
  $node -is [System.Management.Automation.Language.StringConstantExpressionAst]
}, $true) | ForEach-Object Value)
$environment = @($strings | Where-Object { $_ -like 'NOVOVM_NODE_MODE=*' } | ForEach-Object { $_.Replace("`r`n", "`n") })
if ($environment.Count -ne 1 -or $environment[0] -notmatch '(?m)^NOVOVM_NODE_MODE=native_execution_tick$' -or
    $environment[0] -notmatch '(?m)^NOVOVM_NATIVE_EXECUTION_CONFIG=/etc/novovm/native-node.json$' -or
    $environment[0] -notmatch '(?m)^NOVOVM_NATIVE_EXECUTION_START=existing$' -or
    $environment[0] -match '(?m)^NOVOVM_PRODUCT_MAINLINE_OVERLAY_' -or
    $environment[0] -match 'RPC_BIND|ALLOW_LEGACY_HOST_EXECUTION') {
  throw 'Template does not select the unique native owner and safe existing restart mode'
}
$checks++
$nativeConfig = @($strings | Where-Object { $_ -like '*"profile": "native_execution_tick"*' })
if ($nativeConfig.Count -ne 1) { throw 'Expected one current native config template' }
$config = $nativeConfig[0] | ConvertFrom-Json
if ($config.execution_backend -ne 'cpu_callbacks' -or $null -ne $config.genesis -or
    $config.peers.Count -ne 0 -or $config.PSObject.Properties.Name -contains 'rpc_addr') {
  throw 'Config must require reviewed genesis/peers, retain CPU backend and omit HTTP'
}
$checks++
if ($ast.Extent.Text -match '\bRemove-Item\b' -or $ast.Extent.Text -match 'target\\\$Target\\release') {
  throw 'Recursive deletion or guessed Cargo target source returned'
}
$checks++
$stageIndex = $ast.Extent.Text.IndexOf('$packagePath = Resolve-PackageDestination')
$publishIndex = $ast.Extent.Text.LastIndexOf('Publish-LinuxPackage $repo')
$buildIndex = $ast.Extent.Text.IndexOf('$buildMessages = @(cargo build')
$checksumIndex = $ast.Extent.Text.LastIndexOf('Copied AOEM runtime differs')
if ($buildIndex -ge $stageIndex -or $stageIndex -ge $checksumIndex -or $checksumIndex -ge $publishIndex) {
  throw 'Validate/build/stage/publish order regressed'
}
$checks++
Write-Output "PASS: $checks package safety/artifact/template assertions; no builds or deletes. Fixtures retained: $fixture"
