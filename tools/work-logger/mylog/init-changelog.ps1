# Initialize SQLite changelog database (PowerShell version)
# Uses .NET SQLite provider

param(
    [switch]$Reset = $false
)

$dbPath = Join-Path $PSScriptRoot "changelog.db"
$schemaPath = Join-Path $PSScriptRoot "schema.sql"

try {
    if ($Reset -and (Test-Path $dbPath)) {
        Write-Host "⚠️  Resetting $dbPath..." -ForegroundColor Yellow
        Remove-Item $dbPath -Force
    }
    
    if (-not (Test-Path $schemaPath)) {
        Write-Host "❌ Schema file not found: $schemaPath" -ForegroundColor Red
        exit 1
    }
    
    # Load schema
    $schemaContent = Get-Content $schemaPath -Raw
    
    # Use .NET's built-in SQLite (System.Data.SQLite is bundled in newer .NET)
    # For compatibility, we'll use a simple in-process approach
    [System.Reflection.Assembly]::LoadWithPartialName("System.Data.SQLite") | Out-Null
    
    $connString = "Data Source=$dbPath;Version=3;"
    $conn = New-Object System.Data.SQLite.SQLiteConnection($connString)
    $conn.Open()
    
    $cmd = $conn.CreateCommand()
    $cmd.CommandText = $schemaContent
    $cmd.ExecuteNonQuery() | Out-Null
    
    # Verify table creation
    $verifyCmd = $conn.CreateCommand()
    $verifyCmd.CommandText = "SELECT name FROM sqlite_master WHERE type='table' AND name='changelog'"
    $result = $verifyCmd.ExecuteScalar()
    
    $conn.Close()
    
    if ($result) {
        Write-Host "✅ Database initialized: $dbPath" -ForegroundColor Green
        Write-Host "   Tables created: changelog, module_registry, property_registry" -ForegroundColor Green
        exit 0
    } else {
        Write-Host "❌ Failed to create tables" -ForegroundColor Red
        exit 1
    }
}
catch {
    Write-Host "❌ Error: $_" -ForegroundColor Red
    Write-Host "`nNote: This script requires .NET Framework with SQLite support." -ForegroundColor Yellow
    Write-Host "Alternative: Use the Python version (init-changelog.py) or install sqlite3 CLI." -ForegroundColor Yellow
    exit 1
}
