# One deployment operation, not a second node/runtime. All key work uses the packaged ctl.
#Requires -RunAsAdministrator
param(
    [Parameter(Mandatory=$true)][ValidateSet('A','C','D')][string]$HostId,
    [Parameter(Mandatory=$true)][string]$PackageRoot
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$source = (Resolve-Path -LiteralPath $PackageRoot).Path.TrimEnd('\')
$release = 'C:\ProgramData\NOVOVM\releases\selfrun-9a6153f1-windows-x86_64'
$state = 'C:\ProgramData\NOVOVM\selfrun\91129655606029\'+$HostId
$evidence = $PSScriptRoot
$taskName = 'NOVOVM-'+$HostId+'-KeyPreparation-91129655606029'
$manifestPin = '58c92085ca738c1e3e280960df9f16d5432ff38fabcf08fa1dca170b905eff5d'
$resultPath = Join-Path $evidence ($HostId.ToLowerInvariant()+'-provision-result.json')
$result = [ordered]@{ action='install-program-and-service-purpose-keys'; success=$false; chain_id=91129655606029; release=$release; state=$state; service_account='NT AUTHORITY\LocalService'; service_installed=$false; node_started=$false; genesis_created=$false; private_material_exported=$false }
function Test-Release([string]$root) {
    if ((Get-FileHash -LiteralPath (Join-Path $root 'release-manifest.json') -Algorithm SHA256).Hash.ToLowerInvariant() -ne $manifestPin) { throw 'Pinned release manifest mismatch' }
    $manifest = Get-Content -LiteralPath (Join-Path $root 'release-manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($manifest.git_commit -ne '9a6153f1eebda4da5c10829abf77575aa106ba8f') { throw 'Unexpected source' }
    foreach ($file in $manifest.artifacts) {
        $full = [IO.Path]::GetFullPath((Join-Path $root $file.path))
        if (-not $full.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Artifact escaped release' }
        $item = Get-Item -LiteralPath $full -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $item.Length -ne $file.bytes) { throw "Artifact shape mismatch: $($file.path)" }
        if ((Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) { throw "Artifact hash mismatch: $($file.path)" }
    }
    return $manifest.artifacts.Count
}
try {
    if (Test-Path -LiteralPath $resultPath) { throw 'Existing deployment result: inspect before retry' }
    if ((Test-Path -LiteralPath $release) -or (Test-Path -LiteralPath $state)) { throw 'Release/state already exists: no automatic overwrite or regeneration' }
    if (Get-Service -Name ('NOVOVM-'+$HostId) -ErrorAction SilentlyContinue) { throw 'Service already exists: refusing implicit adoption' }
    if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) { throw 'Provisioning task already exists' }
    foreach ($parent in @('C:\ProgramData', 'C:\ProgramData\NOVOVM', 'C:\ProgramData\NOVOVM\releases', 'C:\ProgramData\NOVOVM\selfrun', 'C:\ProgramData\NOVOVM\selfrun\91129655606029')) {
        if ((Test-Path -LiteralPath $parent) -and ((Get-Item -LiteralPath $parent -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Reparse parent: $parent" }
    }
    $result['source_artifacts_verified'] = Test-Release $source
    New-Item -ItemType Directory -Path (Split-Path -Parent $release) -Force | Out-Null
    Copy-Item -LiteralPath $source -Destination $release -Recurse
    $result['installed_artifacts_verified'] = Test-Release $release
    # Files need effective ACEs without directory-only inheritance flags.
    foreach ($entry in @((Get-Item -LiteralPath $release -Force)) + @(Get-ChildItem -LiteralPath $release -Recurse -Force)) {
        if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked release entry' }
        $acl = if ($entry.PSIsContainer) { New-Object Security.AccessControl.DirectorySecurity } else { New-Object Security.AccessControl.FileSecurity }
        $inherit = if ($entry.PSIsContainer) { [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit' } else { [Security.AccessControl.InheritanceFlags]::None }
        $acl.SetAccessRuleProtection($true,$false)
        foreach ($sidText in @('S-1-5-18','S-1-5-32-544','S-1-5-19')) {
            $sid = New-Object Security.Principal.SecurityIdentifier($sidText)
            $rights = if ($sidText -eq 'S-1-5-19') { [Security.AccessControl.FileSystemRights]::ReadAndExecute } else { [Security.AccessControl.FileSystemRights]::FullControl }
            $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($sid,$rights,$inherit,[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow)))
        }
        Set-Acl -LiteralPath $entry.FullName -AclObject $acl
    }
    New-Item -ItemType Directory -Path $state | Out-Null
    & icacls.exe $state /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)(F)' '*S-1-5-32-544:(OI)(CI)(F)' '*S-1-5-19:(OI)(CI)(F)' /T /Q | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Private state ACL failed' }
    $taskScript = Join-Path $state 'prepare-purpose-keys.ps1'
    # This only invokes existing key/qualification owners; node runtime remains SCM.
    $taskBody = @'
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$release='@@RELEASE@@'
$state='@@STATE@@'
$env:AOEM_DLL_MANIFEST=Join-Path $release 'aoem\manifest\aoem-manifest.json'
$env:NOVOVM_AOEM_MANIFEST=$env:AOEM_DLL_MANIFEST
$env:AOEM_DLL_MANIFEST_REQUIRED='1'
$ctl=Join-Path $release 'bin\novovmctl.exe'
$lib=Join-Path $release 'aoem\windows\core\bin\aoem_ffi.dll'
$pin='1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50'
$report=[ordered]@{ success=$false; sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value; name=[Security.Principal.WindowsIdentity]::GetCurrent().Name; node_started=$false; genesis_created=$false; private_material_exported=$false }
try {
    if ($report.sid -ne 'S-1-5-19') { throw 'Not the intended LocalService account' }
    & $ctl native-identity qualify-runtime --library $lib --library-sha256 $pin 1> (Join-Path $state 'runtime.json') 2> (Join-Path $state 'runtime.stderr')
    if ($LASTEXITCODE -ne 0) { throw 'Actual service-account runtime qualification failed' }
    foreach ($purpose in @('validator','transport')) {
        $key=Join-Path $state ($purpose+'.protected')
        & $ctl native-identity generate --key-file $key --purpose $purpose --chain-id 91129655606029 --library $lib --library-sha256 $pin 1> (Join-Path $state ($purpose+'-generate.json')) 2> (Join-Path $state ($purpose+'-generate.stderr'))
        if ($LASTEXITCODE -ne 0) { throw "Protected generation failed: $purpose" }
        & $ctl native-identity inspect --key-file $key --purpose $purpose --chain-id 91129655606029 --library $lib --library-sha256 $pin 1> (Join-Path $state ($purpose+'-inspect.json')) 2> (Join-Path $state ($purpose+'-inspect.stderr'))
        if ($LASTEXITCODE -ne 0) { throw "Protected readback failed: $purpose" }
    }
    $report.success=$true
} catch { $report['error']=$_.Exception.Message }
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $state 'key-preparation-result.json') -Encoding UTF8
if (-not $report.success) { exit 1 }
'@
    $taskBody=$taskBody.Replace('@@RELEASE@@',$release).Replace('@@STATE@@',$state)
    Set-Content -LiteralPath $taskScript -Value $taskBody -Encoding UTF8
    $action = New-ScheduledTaskAction -Execute 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' -Argument ('-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File "'+$taskScript+'"')
    $principal = New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 3) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings | Out-Null
    Start-ScheduledTask -TaskName $taskName
    $deadline = (Get-Date).AddMinutes(3)
    do {
        Start-Sleep -Milliseconds 500
        $task = Get-ScheduledTask -TaskName $taskName
        $info = Get-ScheduledTaskInfo -TaskName $taskName
        if ((Test-Path -LiteralPath (Join-Path $state 'key-preparation-result.json')) -and $task.State -ne 'Running') { break }
    } while ((Get-Date) -lt $deadline)
    if ($task.State -eq 'Running') { throw 'Purpose preparation timed out; preserve task and files for inspection' }
    $keyResult = Get-Content -LiteralPath (Join-Path $state 'key-preparation-result.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    $result['key_preparation']=$keyResult
    $result['task_exit_code']=$info.LastTaskResult
    if (-not $keyResult.success -or $keyResult.sid -ne 'S-1-5-19' -or $info.LastTaskResult -ne 0) { throw 'Service-account purpose preparation failed; preserve original files' }
    foreach ($name in @('runtime.json','validator-generate.json','validator-inspect.json','transport-generate.json','transport-inspect.json','key-preparation-result.json')) {
        Copy-Item -LiteralPath (Join-Path $state $name) -Destination (Join-Path $evidence ($HostId.ToLowerInvariant()+'-'+$name))
    }
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    $result['temporary_task_removed']=$true
    $result.success=$true
} catch { $result['error']=$_.Exception.Message }
$result | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $resultPath -Encoding UTF8
if (-not $result.success) { exit 1 }
