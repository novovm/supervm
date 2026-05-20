<#
DEPRECATED / NON-PROD ENTRY (DISABLED)

This legacy script is decommissioned under NOVOVM single-mainline policy.
SUPERVM is retained only as the repository/path/internal historical code name.
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

throw "DISABLED: scripts/novovm-prod-daemon.ps1 is decommissioned under NOVOVM single-mainline policy. SUPERVM is only an internal historical code name. Use 'novovmctl daemon' (or scripts/novovm-up.ps1)."
