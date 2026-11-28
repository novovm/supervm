# Exercise the real installer with a detached package and mocked Windows task manager.
$ErrorActionPreference = 'Stop'
$parent = (Resolve-Path -LiteralPath ([IO.Path]::GetTempPath())).ProviderPath.TrimEnd('\', '/')
$temp = Join-Path $parent ('novovm-renewal-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null
$local = Join-Path $temp 'LocalAppData'
$installerPath = Join-Path $PSScriptRoot '../windows/install-novovm-client.ps1'
$source = Get-Content -Raw -LiteralPath $installerPath
$source = $source.Replace("[Environment]::GetFolderPath('LocalApplicationData')", '(Get-TestLocalAppData)')
$installer = [scriptblock]::Create($source)
$program = 'bin/novovm-native-client.exe'
$profilePath = 'config/discovery/installed-discovery.json'
$runtime = 'aoem/windows/core/bin/aoem_ffi.dll'
$mocked = [Collections.Generic.List[string]]::new()
$operations = [Collections.Generic.List[string]]::new()
$task = $null
$rejectPreflight = $false
$failSet = $false

function Get-TestLocalAppData { $local }
function Get-ScheduledTask { $script:task }
function Export-ScheduledTask {
    param($TaskName, $TaskPath)
    if ($TaskName -ne $task.TaskName -or $TaskPath -ne '\') { throw 'Unexpected task export' }
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    "<Task xmlns='http://schemas.microsoft.com/windows/2004/02/mit/task'><Principals><Principal><UserId>$sid</UserId></Principal></Principals></Task>"
}
function New-ScheduledTaskAction {
    param($Execute, $Argument, $WorkingDirectory)
    [pscustomobject]@{ Execute=$Execute; Arguments=$Argument; WorkingDirectory=$WorkingDirectory }
}
function Should-UseTestTask([string]$Name, [string]$Path) {
    if ($Name -ne $task.TaskName -or $Path -ne '\') { throw 'Unexpected task mutation' }
}
function Stop-ScheduledTask {
    param($TaskName, $TaskPath)
    Should-UseTestTask $TaskName $TaskPath
    $operations.Add('stop')
    $task.State = 'Ready'
}
function Set-ScheduledTask {
    param($TaskName, $TaskPath, $Action)
    Should-UseTestTask $TaskName $TaskPath
    if ($failSet) { throw 'injected task update failure' }
    $operations.Add('set')
    $task.Actions = @($Action)
}
function Start-ScheduledTask {
    param($TaskName, $TaskPath)
    Should-UseTestTask $TaskName $TaskPath
    $operations.Add('start')
    $task.State = 'Running'
}
function Register-ScheduledTask { throw 'Renewal must not register a task.' }

function Write-FixtureRelease([string]$Name, [int64]$Expiry, [int]$Block) {
    $root = Join-Path $temp $Name
    foreach ($path in $program, $profilePath, $runtime) {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent (Join-Path $root $path)) | Out-Null
    }
    Set-Content -LiteralPath (Join-Path $root $program) -Value 'installer-contract fixture' -Encoding ascii
    Set-Content -LiteralPath (Join-Path $root $runtime) -Value 'runtime-contract fixture' -Encoding ascii
    [ordered]@{
        library = '../../' + $runtime
        library_sha256 = (Get-FileHash -LiteralPath (Join-Path $root $runtime)).Hash.ToLowerInvariant()
        seeds = @(@{contact=@{endpoint_key=@(1)}}, @{contact=@{endpoint_key=@(2)}})
        verification = @{minimum_finalized_checkpoint=[ordered]@{height=2; block_hash=@($Block); valid_until_unix_ms=$Expiry}}
    } | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $root $profilePath) -Encoding utf8
    $artifacts = foreach ($path in $program, $profilePath, $runtime) {
        @{path=$path; sha256=(Get-FileHash -LiteralPath (Join-Path $root $path)).Hash.ToLowerInvariant()}
    }
    $manifest = Join-Path $root 'release-manifest.json'
    @{target='x86_64-pc-windows-msvc'; artifacts=@($artifacts)} | ConvertTo-Json -Depth 8 |
        Set-Content -LiteralPath $manifest -Encoding utf8
    $mock = 'Function:\global:' + (Join-Path $root $program)
    Set-Item -LiteralPath $mock -Value {
        param([Parameter(ValueFromRemainingArguments)]$NativeArguments)
        if ($NativeArguments[1] -ne 'check-installed-profile') { throw 'Missing renewal preflight' }
        $global:LASTEXITCODE = if ($script:rejectPreflight) { 1 } else { 0 }
    }
    $mocked.Add($mock)
    @{PackageRoot=$root; ReleaseManifestSha256=(Get-FileHash -LiteralPath $manifest).Hash.ToLowerInvariant()}
}

try {
    $old = Write-FixtureRelease old 5 42
    $oldPlan = & $installer -Action Plan @old | ConvertFrom-Json
    New-Item -ItemType Directory -Path (Split-Path -Parent $oldPlan.installed_release) -Force | Out-Null
    Copy-Item -LiteralPath $old.PackageRoot -Destination $oldPlan.installed_release -Recurse
    $cache = Join-Path $local 'client-state-sentinel'
    Set-Content -LiteralPath $cache -Value 'existing state'
    $task = [pscustomobject]@{
        TaskName=$oldPlan.task; State='Running';
        Principal=[pscustomobject]@{LogonType=3; RunLevel=0}
        Actions=@((New-ScheduledTaskAction $oldPlan.program $oldPlan.arguments $oldPlan.working_directory))
        Triggers=@([pscustomobject]@{CimClass=[pscustomobject]@{CimClassName='MSFT_TaskLogonTrigger'}})
    }
    $wrong = Write-FixtureRelease wrong 10 43
    $new = Write-FixtureRelease renewed 10 42
    $renewal = @{RenewProfileFromManifestSha256=$old.ReleaseManifestSha256}
    try {
        & $installer @wrong @renewal | Out-Null
        throw 'Another checkpoint was accepted'
    } catch { if ($_.Exception.Message -notlike '*only extend the same pinned checkpoint*') { throw } }
    $rejectPreflight = $true
    try {
        & $installer @new @renewal | Out-Null
        throw 'Rejected profile was accepted'
    } catch { if ($_.Exception.Message -notlike '*rejected the issued discovery profile*') { throw } }
    if ($operations.Count -or $task.State -ne 'Running') { throw 'Rejected renewal touched the previous task' }
    $rejectPreflight = $false
    $failSet = $true
    try {
        & $installer @new @renewal | Out-Null
        throw 'Injected failure did not reach the installer'
    } catch { if ($_.Exception.Message -ne 'injected task update failure') { throw } }
    $failSet = $false
    $plan = & $installer @new @renewal | ConvertFrom-Json
    $repeat = & $installer @new @renewal | ConvertFrom-Json
    if (($operations -join ',') -ne 'stop,set,start' -or $task.State -ne 'Running' -or
        $task.Actions[0].Execute -ne $plan.program -or $repeat.program -ne $plan.program -or
        (Get-Content -Raw -LiteralPath $cache).Trim() -ne 'existing state') {
        throw 'Renewal did not resume the exact task while preserving local state'
    }
} finally {
    foreach ($mock in $mocked) { Remove-Item -LiteralPath $mock -ErrorAction SilentlyContinue }
    $resolved = (Resolve-Path -LiteralPath $temp).ProviderPath
    if ($resolved -ne $temp -or (Split-Path -Parent $resolved) -ne $parent) { throw 'Unexpected fixture cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
'PASS: Windows ordinary Client profile renewal preflights, preserves state, and retries safely'
