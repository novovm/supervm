param(
    [string]$RepoOwner = "novovm",
    [string]$RepoName = "supervm",
    [string]$Branch = "main",
    [string]$Token = "",
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $Token) {
    $Token = $env:GITHUB_TOKEN
}
if (-not $Token) {
    throw "missing token: pass -Token or set GITHUB_TOKEN"
}

$requiredChecks = @(
    "Rust checks",
    "Governance RPC gate (vote verifier)"
)

$checksPayload = @()
foreach ($name in $requiredChecks) {
    $checksPayload += [ordered]@{
        context = $name
        app_id = -1
    }
}

$body = [ordered]@{
    required_status_checks = [ordered]@{
        strict = $true
        checks = $checksPayload
    }
    enforce_admins = $false
    required_pull_request_reviews = $null
    restrictions = $null
}

$bodyJson = $body | ConvertTo-Json -Depth 10
$apiBase = "https://api.github.com/repos/$RepoOwner/$RepoName/branches/$Branch/protection"

Write-Host "branch protection target: $RepoOwner/$RepoName@$Branch"
Write-Host "required checks: $($requiredChecks -join ', ')"

if ($DryRun) {
    Write-Host "dry-run payload:"
    Write-Host $bodyJson
    exit 0
}

$headers = @{
    Authorization = "Bearer $Token"
    Accept = "application/vnd.github+json"
    "X-GitHub-Api-Version" = "2022-11-28"
}

Write-Host "applying branch protection..."
Invoke-RestMethod `
    -Method Put `
    -Uri $apiBase `
    -Headers $headers `
    -ContentType "application/json" `
    -Body $bodyJson | Out-Null

Write-Host "verifying branch protection..."
$verify = Invoke-RestMethod `
    -Method Get `
    -Uri $apiBase `
    -Headers $headers

$applied = @()
if ($verify.required_status_checks -and $verify.required_status_checks.checks) {
    foreach ($item in $verify.required_status_checks.checks) {
        $applied += [string]$item.context
    }
}

foreach ($name in $requiredChecks) {
    if (-not ($applied -contains $name)) {
        throw "required check not applied: $name"
    }
}

Write-Host "branch protection applied successfully."
Write-Host "applied checks: $($applied -join ', ')"
