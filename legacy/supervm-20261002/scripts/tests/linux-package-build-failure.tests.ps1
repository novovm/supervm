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
$bins = @('first', 'second')
$Target = 'x86_64-unknown-linux-gnu'
$script:buildExit = 1
$script:calls = 0
function cargo {
  $script:calls++
  $global:LASTEXITCODE = $script:buildExit
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
Write-Output 'PASS: failed cargo stops packaging; successful builds continue'
exit 0
