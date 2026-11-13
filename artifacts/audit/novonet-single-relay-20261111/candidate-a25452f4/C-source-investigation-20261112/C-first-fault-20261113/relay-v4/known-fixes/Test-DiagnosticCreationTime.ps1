#Requires -Version 7.0
$ErrorActionPreference='Stop'
function Matches([DateTime]$Actual,$Recorded){$Actual.ToUniversalTime().Ticks -eq ([DateTime]$Recorded).ToUniversalTime().Ticks}
$utc=[DateTime]'2026-11-13T08:52:00.0792165Z'
$fromJson=('{"created":"2026-11-13T08:52:00.0792165Z"}'|ConvertFrom-Json).created
if($fromJson -isnot [DateTime]){throw 'This regression requires actual DateTime JSON parsing'}
$cases=@(
 @{name='old_ISO_text_comparison_reproduces_false';pass=($utc.ToString('o') -cne $fromJson)},
 @{name='JSON_DateTime_matches_same_creation';pass=(Matches $utc $fromJson)},
 @{name='ISO_string_matches_same_creation';pass=(Matches $utc '2026-11-13T08:52:00.0792165Z')},
 @{name='offset_time_matches_same_instant';pass=(Matches $utc '2026-11-13T16:52:00.0792165+08:00')},
 @{name='reused_PID_different_creation_rejected';pass=(!(Matches $utc $utc.AddTicks(1)))}
)
if(@($cases|Where-Object {!$_.pass}).Count){throw 'Timestamp regression failed'}
$t=$null;$e=$null;[void][Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'Start-FirstFaultWithRelay-v4.ps1'),[ref]$t,[ref]$e)
if($e.Count){throw 'Launcher AST failed'}
@{utc=[DateTime]::UtcNow.ToString('o');cases=$cases;launcher_AST='PASS';processes_stopped=$false;node_launched=$false}|ConvertTo-Json -Depth 6|Set-Content (Join-Path $PSScriptRoot 'creation-time-regression-public.json')
'5 timestamp cases and AST passed; no process action'
