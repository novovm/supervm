param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)

$ErrorActionPreference = "Stop"
$name = Split-Path -Leaf $PSCommandPath
$msg = "DISABLED: $name has been decommissioned under production-only policy. Use scripts/migration/run_prod_node_e2e_tps.ps1 with novovm-node (ffi_v2)."
Write-Error $msg
exit 1
