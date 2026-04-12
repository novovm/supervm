<#
DEPRECATED / NON-PROD ENTRY (DISABLED)

This legacy script is decommissioned under SuperVM single-mainline policy.
Production entry is only:
  novovmctl daemon

Use:
  scripts/novovm-up.ps1
or call novovmctl directly.
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

throw "DISABLED: scripts/novovm-prod-daemon.ps1 is decommissioned under single-mainline policy. Use 'novovmctl daemon' (or scripts/novovm-up.ps1)."
