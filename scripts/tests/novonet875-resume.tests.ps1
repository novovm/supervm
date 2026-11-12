#Requires -Version 7.0
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$path=Join-Path $PSScriptRoot '../windows/Resume-Novonet875C.ps1'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Resolve-Path $path),[ref]$tokens,[ref]$errors)
if($errors.Count){throw ($errors|Out-String)}
foreach($def in $ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$false)){. ([scriptblock]::Create($def.Extent.Text))}
$original=[Management.Automation.Language.Parser]::ParseFile((Resolve-Path "$PSScriptRoot/../windows/Update-NovonetExisting.ps1"),[ref]$tokens,[ref]$errors)
foreach($def in $original.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -in @('Assert-NoRedirect','Write-NewJson')},$false)){. ([scriptblock]::Create($def.Extent.Text))}
$script:checks=0
function Reject([scriptblock]$Action){$did=$false;try{& $Action}catch{$did=$true};if(!$did){throw 'Unsafe case accepted'};$script:checks++}
function Fixture {
    [pscustomobject]@{TaskName='NOVOVM-C-875d3307-Existing-20261111';State='Ready';Principal=[pscustomobject]@{UserId='S-1-5-19'};Settings=[pscustomobject]@{RestartCount=0};Triggers=@();Actions=@([pscustomobject]@{Execute='C:\Program Files\PowerShell\7\pwsh.exe';Arguments='-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File "C:\fixed\ExistingOwner.ps1" -HostId C -Owner'})}
}
$info=[pscustomobject]@{LastTaskResult=3221225477}
Assert-C875Task (Fixture) $info 'C:\fixed\ExistingOwner.ps1';$script:checks++
foreach($change in @('running','oldtask','system','restart','trigger','initialize','exe','twoactions','result')){
    $task=Fixture;$i=$info.PSObject.Copy()
    switch($change){
        'running'{$task.State='Running'}
        'oldtask'{$task.TaskName='NOVOVM-C-8fed-Existing'}
        'system'{$task.Principal.UserId='S-1-5-18'}
        'restart'{$task.Settings.RestartCount=1}
        'trigger'{$task.Triggers=@('scheduled')}
        'initialize'{$task.Actions[0].Arguments+=' -Initialize'}
        'exe'{$task.Actions[0].Execute='cmd.exe'}
        'twoactions'{$task.Actions+= $task.Actions[0]}
        'result'{$i.LastTaskResult=0}
    }
    Reject {Assert-C875Task $task $i 'C:\fixed\ExistingOwner.ps1'}
}
$crash=[pscustomobject]@{exception_code='c0000005';fault_pid=31296;record_id=546255;event_utc=[DateTime]::UtcNow.AddMinutes(-5)}
$exit=[pscustomobject]@{start='existing';exit_code=-1073741819;time=$crash.event_utc.AddSeconds(2).ToString('o')}
Assert-C875Exit $exit $crash;$script:checks++
$second=$crash.PSObject.Copy();$second.fault_pid=3228;$second.record_id=546354
Assert-C875Exit $exit $second 3228;$script:checks++
Reject {Assert-C875Exit $exit $second 31296}
Reject {Assert-C875Exit $exit $crash 3228}
Reject {Assert-C875Exit $exit $second 123}
$wrongRecord=$second.PSObject.Copy();$wrongRecord.record_id=546255
Reject {Assert-C875Exit $exit $wrongRecord 3228}
foreach($change in @('start','code','exception','pid','earlier','future')){
    $c=$crash.PSObject.Copy();$e=$exit.PSObject.Copy()
    switch($change){'start'{$e.start='initialize'} 'code'{$e.exit_code=0} 'exception'{$c.exception_code='c0000374'} 'pid'{$c.fault_pid=123} 'earlier'{$e.time=$c.event_utc.AddSeconds(-6).ToString('o')} 'future'{$e.time=[DateTime]::UtcNow.AddMinutes(10).ToString('o')}}
    Reject {Assert-C875Exit $e $c}
}
$root=Join-Path $PSScriptRoot ('../../target/resume875-tests-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path "$root/logs" -Force|Out-Null
Write-NewJson "$root/logs/exit.json" $exit
$hash=(Get-FileHash "$root/logs/exit.json").Hash
$sddl=(Get-Acl "$root/logs").Sddl
$dest=Join-Path $root ('logs-crash-875-'+[Guid]::NewGuid().ToString('N'))
Reject {Move-C875Logs $root (Join-Path (Split-Path $root) 'logs-crash-875-11111111111111111111111111111111')}
Reject {Move-C875Logs $root (Join-Path $root 'chain-state')}
Move-C875Logs $root $dest
if((Get-FileHash "$dest/exit.json").Hash -cne $hash -or @(Get-ChildItem "$root/logs").Count -ne 0 -or (Get-Acl "$root/logs").Sddl -cne $sddl){throw 'Log move lost bytes, empty owner directory or ACL'}
$script:checks++
Reject {Move-C875Logs $root $dest}
$commands=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.CommandAst]},$true))
if(@($commands|Where-Object {$_.GetCommandName() -in @('Remove-Item','Stop-Process','Stop-ScheduledTask','Register-ScheduledTask','Set-NetFirewallRule')}).Count){throw 'Unexpected destructive or service mutation'}
if(@($commands|Where-Object {$_.GetCommandName() -eq 'Start-ScheduledTask'}).Count -ne 1){throw 'Must have only one restart call'}
$script:checks++
Write-Output "PASS $script:checks C875 same-owner recovery checks; no node/task/database operated"
