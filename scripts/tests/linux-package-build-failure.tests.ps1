# Exercise the actual packaging build loop without building or replacing a package.
$ErrorActionPreference = 'Stop'
$path = Join-Path $PSScriptRoot '../novovm-package-product-linux.ps1'
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
  (Resolve-Path $path).Path, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw $parseErrors }
$loops = $ast.FindAll({
  param($node)
  $node -is [System.Management.Automation.Language.ForEachStatementAst] -and
    $node.Extent.Text.Contains('cargo build')
}, $true)
if ($loops.Count -ne 1) { throw 'Expected one package build loop' }
$build = [scriptblock]::Create($loops[0].Extent.Text)
$helpers = $ast.FindAll({ param($node)
  $node -is [System.Management.Automation.Language.FunctionDefinitionAst]
}, $true)
foreach ($helper in $helpers) { . ([scriptblock]::Create($helper.Extent.Text)) }
$fixture = Join-Path $PSScriptRoot ('../../artifacts/audit/linux-build-fixture-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
$fixture = (Resolve-Path $fixture).Path
$executable = Join-Path $fixture 'reported-linux-binary'
[IO.File]::WriteAllBytes($executable, [byte[]]@(0x7f, 0x45, 0x4c, 0x46))
$bins = @('first', 'second')
$executables = @{}
$Target = 'x86_64-unknown-linux-gnu'
$script:buildExit = 1
$script:calls = 0
function cargo {
  $script:calls++
  $global:LASTEXITCODE = $script:buildExit
  if ($script:buildExit -eq 0) {
    [PSCustomObject]@{reason='compiler-artifact';target=@{name=$bin;kind=@('bin')};executable=$executable} | ConvertTo-Json -Compress
  }
}
$rejected = $false
try { & $build } catch {
  if ($_.Exception.Message -notlike '*refusing to package possibly stale binaries*') { throw }
  $rejected = $true
}
if (!$rejected -or $script:calls -ne 1) {
  throw 'Failed cargo must stop the package build immediately'
}
$script:buildExit = 0
$script:calls = 0
& $build
if ($script:calls -ne 2) { throw 'Each successful binary must be built' }
if ($executables['first'] -ne $executable -or $executables['second'] -ne $executable) {
  throw 'The actual Cargo-reported executable, not a guessed target path, must be consumed'
}
# A successful exit with no artifact cannot silently accept a stale namesake.
function cargo { $global:LASTEXITCODE = 0; '{"reason":"build-finished","success":true}' }
$rejected = $false
try { & $build } catch {
  if ($_.Exception.Message -notlike '*exactly one executable*') { throw }
  $rejected = $true
}
if (!$rejected) { throw 'Successful cargo without a reported executable must fail' }
Write-Output "PASS: build failure and absent artifact reject; Cargo-reported paths consumed. Generated fixtures retained: $fixture"
exit 0
