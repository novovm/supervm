[CmdletBinding()]
param(
    [ValidateSet("backup", "restore", "migrate")]
    [string]$Action = "backup",
    [string]$RepoRoot = "",
    [string]$Snapshot = "",
    [string]$GatewayStoreFrom = "",
    [string]$PluginStoreFrom = "",
    [string]$PluginAuditFrom = "",
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
    }
    return (Resolve-Path $Root).Path
}

function Resolve-FullPath {
    param(
        [string]$Root,
        [string]$Value
    )
    if ([System.IO.Path]::IsPathRooted($Value)) {
        return [System.IO.Path]::GetFullPath($Value)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $Root $Value))
}

function Ensure-ParentDir {
    param([string]$PathValue)
    $parent = Split-Path -Parent $PathValue
    if ($parent -and -not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
}

function Remove-PathIfExists {
    param([string]$PathValue)
    if (Test-Path -LiteralPath $PathValue) {
        Remove-Item -LiteralPath $PathValue -Recurse -Force
    }
}

function Assert-RuntimeStopped {
    param([switch]$Bypass)
    if ($Bypass) {
        return
    }
    $hits = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
            $_.ProcessName -match "^novovm-node$|^novovm-evm-gateway$"
        })
    if ($hits.Count -gt 0) {
        $names = ($hits | Select-Object -ExpandProperty ProcessName | Sort-Object -Unique) -join ","
        throw "restore requires runtime stopped; found active process(es): $names ; rerun with -Force to bypass check"
    }
}

function Resolve-SnapshotDir {
    param(
        [string]$BackupRoot,
        [string]$SnapshotName
    )
    if ($SnapshotName) {
        if ([System.IO.Path]::IsPathRooted($SnapshotName)) {
            return [System.IO.Path]::GetFullPath($SnapshotName)
        }
        return [System.IO.Path]::GetFullPath((Join-Path $BackupRoot $SnapshotName))
    }
    if (-not (Test-Path -LiteralPath $BackupRoot)) {
        throw "backup root not found: $BackupRoot"
    }
    $latest = Get-ChildItem -LiteralPath $BackupRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1
    if ($null -eq $latest) {
        throw "no backup snapshot found under: $BackupRoot"
    }
    return $latest.FullName
}

$repo = Resolve-RootPath -Root $RepoRoot
$backupRoot = Resolve-FullPath -Root $repo -Value "artifacts/migration/unifiedaccount/store-backups"

$targets = @(
    [ordered]@{
        name = "gateway_ua_router"
        relative_path = "artifacts/gateway/unified-account-router.rocksdb"
    },
    [ordered]@{
        name = "plugin_ua_router"
        relative_path = "artifacts/migration/unifiedaccount/ua-plugin-self-guard-router.rocksdb"
    },
    [ordered]@{
        name = "plugin_ua_audit"
        relative_path = "artifacts/migration/unifiedaccount/ua-plugin-self-guard-audit.rocksdb"
    }
)

if ($Action -eq "backup") {
    if (-not (Test-Path -LiteralPath $backupRoot)) {
        New-Item -ItemType Directory -Path $backupRoot -Force | Out-Null
    }
    $snapshotId = Get-Date -Format "yyyyMMdd-HHmmss"
    $snapshotDir = Join-Path $backupRoot $snapshotId
    New-Item -ItemType Directory -Path $snapshotDir -Force | Out-Null

    $records = @()
    foreach ($target in $targets) {
        $src = Resolve-FullPath -Root $repo -Value $target.relative_path
        $dst = Join-Path $snapshotDir $target.relative_path
        $entry = [ordered]@{
            name = $target.name
            source = $src
            destination = $dst
            copied = $false
            existed = $false
        }
        if (Test-Path -LiteralPath $src) {
            $entry.existed = $true
            Ensure-ParentDir -PathValue $dst
            Copy-Item -LiteralPath $src -Destination $dst -Recurse -Force
            $entry.copied = $true
        }
        $records += [pscustomobject]$entry
    }

    $manifest = [ordered]@{
        version = 1
        action = "backup"
        snapshot_id = $snapshotId
        created_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        repo_root = $repo
        targets = $records
    }
    $manifestPath = Join-Path $snapshotDir "manifest.json"
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding UTF8

    $copiedCount = @($records | Where-Object { $_.copied }).Count
    Write-Host "ua_store_backup_out: snapshot_id=$snapshotId copied_targets=$copiedCount snapshot_dir=$snapshotDir manifest=$manifestPath"
    exit 0
}

if ($Action -eq "restore") {
    Assert-RuntimeStopped -Bypass:$Force
    $snapshotDir = Resolve-SnapshotDir -BackupRoot $backupRoot -SnapshotName $Snapshot
    if (-not (Test-Path -LiteralPath $snapshotDir)) {
        throw "snapshot directory not found: $snapshotDir"
    }

    $restored = 0
    foreach ($target in $targets) {
        $src = Join-Path $snapshotDir $target.relative_path
        $dst = Resolve-FullPath -Root $repo -Value $target.relative_path
        if (-not (Test-Path -LiteralPath $src)) {
            continue
        }
        Remove-PathIfExists -PathValue $dst
        Ensure-ParentDir -PathValue $dst
        Copy-Item -LiteralPath $src -Destination $dst -Recurse -Force
        $restored = $restored + 1
    }
    Write-Host "ua_store_restore_out: snapshot_dir=$snapshotDir restored_targets=$restored"
    exit 0
}

if ($Action -eq "migrate") {
    Assert-RuntimeStopped -Bypass:$Force

    $gatewayDst = Resolve-FullPath -Root $repo -Value "artifacts/gateway/unified-account-router.rocksdb"
    $pluginStoreDst = Resolve-FullPath -Root $repo -Value "artifacts/migration/unifiedaccount/ua-plugin-self-guard-router.rocksdb"
    $pluginAuditDst = Resolve-FullPath -Root $repo -Value "artifacts/migration/unifiedaccount/ua-plugin-self-guard-audit.rocksdb"

    $gatewaySrcRaw = if ($GatewayStoreFrom) {
        $GatewayStoreFrom
    } elseif ($env:NOVOVM_GATEWAY_UA_STORE_PATH) {
        $env:NOVOVM_GATEWAY_UA_STORE_PATH
    } else {
        "artifacts/gateway/unified-account-router.bin"
    }
    $pluginStoreSrcRaw = if ($PluginStoreFrom) {
        $PluginStoreFrom
    } elseif ($env:NOVOVM_ADAPTER_PLUGIN_UA_STORE_PATH) {
        $env:NOVOVM_ADAPTER_PLUGIN_UA_STORE_PATH
    } else {
        "artifacts/migration/unifiedaccount/ua-plugin-self-guard-router.bin"
    }
    $pluginAuditSrcRaw = if ($PluginAuditFrom) {
        $PluginAuditFrom
    } elseif ($env:NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_PATH) {
        $env:NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_PATH
    } else {
        "artifacts/migration/unifiedaccount/ua-plugin-self-guard-audit.jsonl"
    }

    $pairs = @(
        [ordered]@{
            name = "gateway_ua_router"
            source = (Resolve-FullPath -Root $repo -Value $gatewaySrcRaw)
            destination = $gatewayDst
        },
        [ordered]@{
            name = "plugin_ua_router"
            source = (Resolve-FullPath -Root $repo -Value $pluginStoreSrcRaw)
            destination = $pluginStoreDst
        },
        [ordered]@{
            name = "plugin_ua_audit"
            source = (Resolve-FullPath -Root $repo -Value $pluginAuditSrcRaw)
            destination = $pluginAuditDst
        }
    )

    $migrated = 0
    foreach ($pair in $pairs) {
        $src = [string]$pair.source
        $dst = [string]$pair.destination
        if (-not (Test-Path -LiteralPath $src)) {
            continue
        }
        if ($src -eq $dst) {
            continue
        }
        if ($src.EndsWith(".bin") -or $src.EndsWith(".jsonl")) {
            throw ("legacy codec source is not supported by production migrate action: {0}. use rocksdb source path or keep legacy backend temporarily." -f $src)
        }
        Remove-PathIfExists -PathValue $dst
        Ensure-ParentDir -PathValue $dst
        Copy-Item -LiteralPath $src -Destination $dst -Recurse -Force
        $migrated = $migrated + 1
    }

    Write-Host "ua_store_migrate_out: migrated_targets=$migrated gateway_dst=$gatewayDst plugin_store_dst=$pluginStoreDst plugin_audit_dst=$pluginAuditDst"
    exit 0
}

throw "unsupported action: $Action"
