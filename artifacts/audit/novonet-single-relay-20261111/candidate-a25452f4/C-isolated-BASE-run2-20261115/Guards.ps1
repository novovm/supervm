function Update-BaseCapability($Seen,$Row) {
 if($Row.novonet_present){$Seen[$Row.index]=$true}
}
function Test-BaseLoad($Rows,$Seen) {
 foreach($r in $Rows){
  if(!$Seen.ContainsKey($r.index) -or !$r.transport_present -or $r.reported_data_writes -le 0 -or $r.progress_read_error){return $false}
 }
 return ($Rows.Count -eq 4)
}
function Get-BaseWaitDecision([bool]$ResultExists,[string]$State,[double]$Seconds) {
 if($ResultExists){return 'complete'}
 if($Seconds -gt 45 -and $State -ne 'Running'){return 'failed'}
 return 'wait'
}
