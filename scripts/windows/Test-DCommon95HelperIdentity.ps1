#Requires -Version 7.0
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$tokens=$null;$errors=$null
$entry=Join-Path $PSScriptRoot 'Switch-D-Common95.ps1'
$ast=[Management.Automation.Language.Parser]::ParseFile($entry,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'D entry parse failed'}
$fn=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -ceq 'Get-VerifiedDeploymentHelperText'},$false))
if($fn.Count -ne 1){throw 'Identity helper missing'}
. ([scriptblock]::Create($fn[0].Extent.Text))
$file=[IO.Path]::GetTempFileName()
try {
    $lf="function Example {`n    return 1`n}`n"
    $pin=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($lf))).ToLowerInvariant()
    $cases=@(
        @{name='LF';text=$lf;accept=$true},
        @{name='CRLF';text=$lf.Replace("`n","`r`n");accept=$true},
        @{name='mixed EOL';text=$lf.Replace("return 1`n","return 1`r`n");accept=$true},
        @{name='code mutation';text=$lf.Replace('return 1','return 2');accept=$false},
        @{name='space mutation';text=$lf.Replace('return 1','return  1');accept=$false},
        @{name='bare CR';text=$lf.Replace('return 1',"return`r1");accept=$false},
        @{name='BOM added';text=([string][char]0xfeff)+$lf;accept=$false}
    )
    foreach($case in $cases){
        [IO.File]::WriteAllBytes($file,[Text.Encoding]::UTF8.GetBytes($case.text))
        $rejected=$false;$result=$null
        try {$result=Get-VerifiedDeploymentHelperText $file $pin}
        catch {if($_.Exception.Message -cne 'Original deployment helpers changed'){throw};$rejected=$true}
        if($rejected -eq $case.accept){throw "Unexpected acceptance: $($case.name)"}
        if($case.accept -and $result -cne $lf){throw "Verified content differs: $($case.name)"}
        Write-Output "PASS $($case.name)"
    }
    $null=Get-VerifiedDeploymentHelperText (Join-Path $PSScriptRoot 'Update-NovonetExisting.ps1') '7b50ae0c70ba039dce93ae44b8d18b541e77d3c4ff45e95eedf60c3a0d29d5cd'
    Write-Output 'PASS actual pinned deployment helper; no deployment entry executed'
} finally {Remove-Item -LiteralPath $file}
