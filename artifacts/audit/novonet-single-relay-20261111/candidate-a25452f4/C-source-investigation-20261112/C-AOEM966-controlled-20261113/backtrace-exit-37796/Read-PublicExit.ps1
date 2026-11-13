#Requires -Version 7.0
param([Parameter(Mandatory)][string]$EvidenceRoot)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (Test-Path -LiteralPath $EvidenceRoot) { throw 'Refusing to overwrite evidence' }
New-Item -ItemType Directory -Path $EvidenceRoot | Out-Null
$start = [DateTime]::UtcNow.ToString('o')

function Read-EventFields([xml]$EventXml) {
    $fields = @{}
    # Event XML has a default namespace. Empty Data elements have no #text property.
    foreach ($element in $EventXml.GetElementsByTagName('Data')) {
        $fields[$element.GetAttribute('Name')] = $element.InnerText
    }
    return $fields
}

# Regression for the collector's actual failure: namespaced empty event fields.
$fixture = Read-EventFields ([xml]'<Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event"><EventData><Data Name="AppName">novovm-node.exe</Data><Data Name="Empty" /></EventData></Event>')
if ($fixture['AppName'] -cne 'novovm-node.exe' -or $fixture['Empty'] -cne '' -or $fixture.Count -ne 2) {
    throw 'Event field parser regression'
}
try {
    $events = @(Get-WinEvent -FilterHashtable @{
        LogName = 'Application'; ProviderName = 'Application Error'; Id = 1000
        StartTime = [DateTime]'2026-11-13T08:40:00'
    } | ForEach-Object {
        $fields = Read-EventFields ([xml]$_.ToXml())
        if ($fields['AppName'] -eq 'novovm-node.exe' -and $fields['ProcessId'] -eq '0x93a4') {
            @{
                utc = $_.TimeCreated.ToUniversalTime().ToString('o'); record = $_.RecordId
                application = $fields['AppName']; module = $fields['ModuleName']
                exception = $fields['ExceptionCode']; offset = $fields['FaultingOffset']
                pid = $fields['ProcessId']
            }
        }
    })
    $nodes = @(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'" |
        Select-Object ProcessId, CreationDate)
    $ports = @(Get-NetUDPEndpoint | Where-Object LocalPort -in 41052,42052 |
        Select-Object LocalPort, OwningProcess)
    @{
        host = 'C'; start_utc = $start; end_utc = [DateTime]::UtcNow.ToString('o')
        events = $events; nodes = $nodes; ports = $ports
        parser_regression = 'PASS'; script_exit = 0; read_only = $true
        task_state = 'not collected by this unprivileged supplement'
        node_restarted = $false; original_logs_recopied = $false
    } | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $EvidenceRoot 'public-exit-supplement.json')
    exit 0
} catch {
    @{ start_utc = $start; end_utc = [DateTime]::UtcNow.ToString('o'); script_exit = 1; error = $_.Exception.Message } |
        ConvertTo-Json | Set-Content (Join-Path $EvidenceRoot 'supplement-error.json')
    exit 1
}
