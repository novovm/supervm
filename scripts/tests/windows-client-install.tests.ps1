# Isolated integration check using C's detached database and a real native Client.
# The coordinator prepares the LocalService fixture and temporary Windows tasks.
# The actual installer runs under the signed-in user's non-elevated token.
param(
    [Parameter(Mandatory)][string]$Repository,
    [Parameter(Mandatory)][string]$SnapshotStage,
    [Parameter(Mandatory)][string]$Archive,
    [Parameter(Mandatory)][string]$PreflightPackage,
    [Parameter(Mandatory)][string]$CandidatePackage,
    [Parameter(Mandatory)][scriptblock]$Assemble
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$stage = (Resolve-Path -LiteralPath $SnapshotStage).ProviderPath
$preflight = (Resolve-Path -LiteralPath $PreflightPackage).ProviderPath
$installedPlan = $null
$history = $null
$bootstrapTask = $null
$workerTask = $null
$installStarted = $false
$renamed = $null
$done = $false

function Get-CService {
    Get-CimInstance Win32_Service -Filter "Name='NOVOVM-Native-Validator-C'" |
        Select-Object State, ProcessId | ConvertTo-Json -Compress
}

function Wait-Until([scriptblock]$Condition, [string]$Description, [int]$Seconds = 35) {
    $limit = [DateTimeOffset]::UtcNow.AddSeconds($Seconds)
    do {
        $result = & $Condition
        if ($result) { return $result }
        Start-Sleep -Milliseconds 300
    } while ([DateTimeOffset]::UtcNow -lt $limit)
    throw "Timed out waiting for $Description"
}

function Get-InstalledProcess([string]$Program) {
    Get-CimInstance Win32_Process -Filter "Name='novovm-native-client.exe'" |
        Where-Object ExecutablePath -eq $Program
}

$current = [Security.Principal.WindowsIdentity]::GetCurrent()
$installTask = "NOVOVM-Native-Client-$($current.User.Value)"
if (Get-ScheduledTask -TaskName $installTask -TaskPath '\' -ErrorAction SilentlyContinue) {
    throw 'The current interactive user already has an installed Client task.'
}
$clientRoot = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'NOVOVM/native-discovery'
$before = Get-CService
Write-Output "C_before=$before"
$buildLog = Join-Path $Repository 'target/C-query-slot-goal-test-build.log'
$exeLine = Select-String -LiteralPath $buildLog -SimpleMatch 'Executable unittests' | Select-Object -Last 1
if ($exeLine.Line -notmatch '\(([^()]+novovm_node-[^()]+\.exe)\)') {
    throw 'Compiled Windows install fixture is unavailable.'
}
$test = $Matches[1]
if (![IO.Path]::IsPathRooted($test)) { $test = Join-Path $Repository $test }
Copy-Item -LiteralPath $test -Destination (Join-Path $stage 'query-audit-test.exe')
Copy-Item -LiteralPath (Join-Path $preflight 'bin/novovm-native-client.exe') -Destination (Join-Path $stage 'receipt-proof-client.exe')
$runner = Join-Path $stage 'run.ps1'
$bootstrapTask = 'NOVOVM-Install-Audit-' + (Split-Path -Leaf $stage)
$pwsh = (Resolve-Path -LiteralPath (Join-Path $env:ProgramFiles 'PowerShell/7/pwsh.exe')).ProviderPath
$principal = New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
$action = New-ScheduledTaskAction -Execute $pwsh -Argument ('-NoProfile -NonInteractive -File "' + $runner + '"') -WorkingDirectory $stage
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 10)
Register-ScheduledTask -TaskName $bootstrapTask -Principal $principal -Action $action -Settings $settings | Out-Null

try {
    Start-ScheduledTask -TaskName $bootstrapTask
    $readyPath = Join-Path $stage 'windows-install-ready.json'
    $null = Wait-Until {
        if (Test-Path -LiteralPath (Join-Path $stage 'exit.txt')) { throw 'Detached test owner already ended.' }
        Test-Path -LiteralPath $readyPath
    } 'the detached state fixture'
    $ready = Get-Content -Raw -LiteralPath $readyPath | ConvertFrom-Json
    $profile = Join-Path $stage 'installed-discovery.json'
    $profilePin = (Get-FileHash -LiteralPath $profile).Hash.ToLowerInvariant()
    $profileData = Get-Content -Raw -LiteralPath $profile | ConvertFrom-Json
    $genesis = '0x' + [Convert]::ToHexString([byte[]]$profileData.genesis).ToLowerInvariant()
    $protocol = '0x' + [Convert]::ToHexString([byte[]]$profileData.protocol).ToLowerInvariant()
    $history = Join-Path $clientRoot "$($profileData.chain_id)-$genesis-$protocol"
    if (Test-Path -LiteralPath $history) { throw 'This user already has state for the fixture domain.' }

    & $Assemble $profile $profilePin
    if (Test-Path -LiteralPath (Join-Path $stage 'exit.txt')) { throw 'Detached owner ended before the installation began.' }
    $bundle = (Resolve-Path -LiteralPath $CandidatePackage).ProviderPath
    $manifestPin = (Get-FileHash -LiteralPath (Join-Path $bundle 'release-manifest.json')).Hash
    $installer = Join-Path $bundle 'resources/scripts/windows/install-novovm-client.ps1'
    $installedPlan = & $installer -Action Plan -ReleaseManifestSha256 $manifestPin | ConvertFrom-Json
    if ($installedPlan.task -ne $installTask -or
        (Test-Path -LiteralPath $installedPlan.installed_release) -or (Test-Path -LiteralPath $history)) {
        throw 'Clean installation plan already has persistent state.'
    }
    $taskBefore = @(Get-ScheduledTask -TaskName $installTask -TaskPath '\' -ErrorAction SilentlyContinue).Count
    if ($taskBefore -ne 0) { throw 'Read-only plan registered a task.' }
    $installStarted = $true
    $workerHome = Join-Path $Repository ('target/install-user-' + (Split-Path -Leaf $stage))
    New-Item -ItemType Directory -Path $workerHome | Out-Null
    @{ path=$installer; pin=$manifestPin } | ConvertTo-Json |
        Set-Content -LiteralPath (Join-Path $workerHome 'input.json')
    $worker = Join-Path $workerHome 'install.ps1'
    @'
$ErrorActionPreference = 'Stop'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
try {
    if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Acceptance install unexpectedly received an elevated token.'
    }
    $inputDocument = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'input.json') | ConvertFrom-Json
    & $inputDocument.path -ReleaseManifestSha256 $inputDocument.pin |
        Set-Content -LiteralPath (Join-Path $PSScriptRoot 'plan.json')
    0 | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'exit.txt')
} catch {
    $_ | Out-String | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'error.txt')
    1 | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'exit.txt')
}
'@ | Set-Content -LiteralPath $worker
    $workerTask = $bootstrapTask + '-user'
    $limitedPrincipal = New-ScheduledTaskPrincipal -UserId $current.Name -LogonType Interactive -RunLevel Limited
    $workerAction = New-ScheduledTaskAction -Execute $pwsh -Argument ('-NoProfile -NonInteractive -WindowStyle Hidden -File "' + $worker + '"') -WorkingDirectory $workerHome
    Register-ScheduledTask -TaskName $workerTask -Principal $limitedPrincipal -Action $workerAction | Out-Null
    Start-ScheduledTask -TaskName $workerTask
    $workerExit = Join-Path $workerHome 'exit.txt'
    $null = Wait-Until { Test-Path -LiteralPath $workerExit } 'installation using the non-elevated logon token'
    if ((Get-Content -Raw -LiteralPath $workerExit).Trim() -ne '0') {
        throw (Get-Content -Raw -LiteralPath (Join-Path $workerHome 'error.txt'))
    }
    $installed = Get-Content -Raw -LiteralPath (Join-Path $workerHome 'plan.json') | ConvertFrom-Json
    $task = Get-ScheduledTask -TaskName $installTask -TaskPath '\'
    $trigger = @($task.Triggers)
    if ($installed.program -ne $installedPlan.program -or [int]$task.Principal.RunLevel -ne 0 -or
        [int]$task.Principal.LogonType -ne 3 -or $trigger.Count -ne 1 -or
        $trigger[0].CimClass.CimClassName -ne 'MSFT_TaskLogonTrigger') { throw 'Windows task lost the limited logon startup contract.' }
    $first = Wait-Until { Get-InstalledProcess $installed.program } 'the scheduler-owned Client process'
    $transport = Join-Path $history 'transport.protected'
    $state = Wait-Until {
        if (!(Get-InstalledProcess $installed.program)) { throw 'The installed Client exited during its first sync.' }
        if (!(Test-Path -LiteralPath $history)) { return }
        $chainDir = Get-ChildItem -LiteralPath $history -Directory -Filter 'headers-*' | Select-Object -First 1
        if (!$chainDir) { return }
        $root = Join-Path $chainDir.FullName ('state-nodes/' + $ready.state_root + '.node')
        $header = Join-Path $chainDir.FullName ('{0:d20}.proof' -f [uint64]$ready.height)
        $stateDir = Join-Path $chainDir.FullName 'state-nodes'
        if ((Test-Path -LiteralPath $header) -and (Test-Path -LiteralPath $root) -and
            (Test-Path -LiteralPath $transport) -and
            @(Get-ChildItem -LiteralPath $stateDir -File -Filter '*.node').Count -eq 97) {
            return $chainDir.FullName
        }
    } 'all 97 state nodes at the certified height'
    $identityHash = (Get-FileHash -LiteralPath $transport).Hash
    $installedProfile = Join-Path $installed.installed_release 'config/discovery/installed-discovery.json'
    $proof = & $installed.program $installedProfile verify-uca-execution $profilePin $Archive | ConvertFrom-Json
    if ($proof.height -ne $ready.height -or $proof.block_hash -ne $ready.block_hash -or
        $proof.state_root -ne $ready.state_root -or $proof.state_version -ne $ready.state_version) {
        throw 'The scheduled Client state differs from the isolated certified archive.'
    }
    $null = & $installer -ReleaseManifestSha256 $manifestPin
    if ((Get-InstalledProcess $installed.program).ProcessId -ne $first.ProcessId -or
        (Get-FileHash -LiteralPath $transport).Hash -ne $identityHash) {
        throw 'Re-running the same installer restarted the Client or changed its custody.'
    }
    Write-Output "initial_pid=$($first.ProcessId) height=$($ready.height) state_root=$($ready.state_root) state_nodes=97"
    Stop-ScheduledTask -TaskName $installTask -TaskPath '\'
    $null = Wait-Until { -not (Get-InstalledProcess $installed.program) } 'the controlled test task stop'
    $oneNode = Get-ChildItem -LiteralPath (Join-Path $state 'state-nodes') -File -Filter '*.node' |
        Select-Object -First 1
    $nodeHash = (Get-FileHash -LiteralPath $oneNode.FullName).Hash
    Rename-Item -LiteralPath $oneNode.FullName -NewName ($oneNode.Name + '.held-for-restart')
    $packageParent = (Resolve-Path -LiteralPath (Join-Path $Repository 'artifacts/product-overlay')).ProviderPath
    if ((Split-Path -Parent $bundle) -ne $packageParent) { throw 'Unexpected candidate package path.' }
    $renamed = $bundle + '.acceptance-unavailable'
    Rename-Item -LiteralPath $bundle -NewName (Split-Path -Leaf $renamed)
    Start-ScheduledTask -TaskName $installTask -TaskPath '\'
    $second = Wait-Until { Get-InstalledProcess $installed.program } 'the installed Client restart without extraction'
    if ($second.ProcessId -eq $first.ProcessId -or (Get-FileHash -LiteralPath $transport).Hash -ne $identityHash) {
        throw 'The Windows task restarted with a changed transport key.'
    }
    $null = Wait-Until {
        (Test-Path -LiteralPath $oneNode.FullName) -and
            (Get-FileHash -LiteralPath $oneNode.FullName).Hash -eq $nodeHash
    } 'a fresh authenticated download from the installed copy'
    $proofAgain = & $installed.program $installedProfile verify-uca-execution $profilePin $Archive | ConvertFrom-Json
    if ($proofAgain.block_hash -ne $ready.block_hash -or $proofAgain.state_root -ne $ready.state_root -or
        !(Get-InstalledProcess $installed.program)) { throw 'The detached installation could not reopen its certified state.' }
    Write-Output "restart_pid=$($second.ProcessId) release_manifest_sha256=$manifestPin profile_sha256=$profilePin"

    'verified' | Set-Content -LiteralPath (Join-Path $stage 'windows-install-complete.txt')
    $done = $true
    $null = Wait-Until { Test-Path -LiteralPath (Join-Path $stage 'exit.txt') } 'the detached state fixture to close'
    if ((Get-Content -Raw -LiteralPath (Join-Path $stage 'exit.txt')).Trim() -ne '0') {
        throw 'Detached Windows installation fixture failed after serving.'
    }
    Write-Output "detached_fixture_log_sha256=$((Get-FileHash -LiteralPath (Join-Path $stage 'acceptance.log')).Hash)"
} finally {
    if (!$done) { 'failed' | Set-Content -LiteralPath (Join-Path $stage 'windows-install-complete.txt') }
    if ($workerTask) {
        $task = Get-ScheduledTask -TaskName $workerTask -ErrorAction SilentlyContinue
        if ($task) {
            if ($task.State -eq 'Running') { Stop-ScheduledTask -TaskName $workerTask }
            Unregister-ScheduledTask -TaskName $workerTask -Confirm:$false
        }
    }
    if ($installStarted) {
        $task = Get-ScheduledTask -TaskName $installTask -TaskPath '\' -ErrorAction SilentlyContinue
        if ($task) {
            if ($task.State -eq 'Running') { Stop-ScheduledTask -TaskName $installTask -TaskPath '\' }
            Unregister-ScheduledTask -TaskName $installTask -TaskPath '\' -Confirm:$false
        }
        if ($installedPlan) {
            $null = Wait-Until { -not (Get-InstalledProcess $installedPlan.program) } 'test Client cleanup'
            $label = Split-Path -Leaf $stage
            $release = $installedPlan.installed_release
            if ($release -and (Test-Path -LiteralPath $release)) {
                Rename-Item -LiteralPath $release -NewName ("audit-$label-" + (Split-Path -Leaf $release))
            }
            if ($history -and (Test-Path -LiteralPath $history)) {
                Rename-Item -LiteralPath $history -NewName ("audit-$label-" + (Split-Path -Leaf $history))
            }
        }
    }
    if ($renamed -and (Test-Path -LiteralPath $renamed)) {
        Rename-Item -LiteralPath $renamed -NewName (Split-Path -Leaf $bundle)
    }
    if ($bootstrapTask) {
        $task = Get-ScheduledTask -TaskName $bootstrapTask -ErrorAction SilentlyContinue
        if ($task) {
            if ($task.State -eq 'Running') { Stop-ScheduledTask -TaskName $bootstrapTask }
            Unregister-ScheduledTask -TaskName $bootstrapTask -Confirm:$false
        }
    }
    $after = Get-CService
    Write-Output "C_after=$after"
    if ($before -ne $after) { throw 'C service status changed during the isolated install check.' }
}
