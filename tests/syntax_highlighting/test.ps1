# ── Module and strict mode ───────────────────────────────────────────────────

#Requires -Version 7.0
#Requires -Modules Az.Storage, Az.KeyVault
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ── Variables and hashtables ─────────────────────────────────────────────────

$AppName      = 'Fulgur'
$MaxRetries   = 3
$Threshold    = 0.85
$DryRun       = $false
$StartTime    = Get-Date
$Config       = @{
    LogLevel   = 'Info'
    OutputPath = Join-Path $env:USERPROFILE '.fulgur'
    Tags       = @('syntax', 'highlight', 'powershell')
    Nested     = @{ Enabled = $true; Depth = 5 }
}

# ── Enums ────────────────────────────────────────────────────────────────────

enum Severity {
    Debug
    Info
    Warning
    Error
}

# ── Functions ────────────────────────────────────────────────────────────────

function Invoke-WithRetry {
    param(
        [Parameter(Mandatory)]
        $ScriptBlock,

        [ValidateRange(1, 10)]
        $RetryCount = $MaxRetries,

        $DelayMilliseconds = 200
    )

    for ($attempt = 1; $attempt -le $RetryCount; $attempt++) {
        try {
            return & $ScriptBlock
        }
        catch {
            if ($attempt -eq $RetryCount) { throw }
            Write-Warning "Attempt $attempt failed: $($_.Exception.Message)"
            Start-Sleep -Milliseconds $DelayMilliseconds
        }
    }
}

filter ConvertTo-Slug {
    $_.ToLower().Trim() -replace '[^a-z0-9]+', '-' -replace '^-|-$'
}

function Get-FormattedSize {
    param($Bytes)
    $units = @('B', 'KB', 'MB', 'GB', 'TB')
    $index = 0
    $size  = $Bytes
    while ($size -ge 1024 -and $index -lt $units.Count - 1) {
        $size  = $size / 1024
        $index++
    }
    return '{0:N2} {1}' -f $size, $units[$index]
}

# ── Pipeline and operators ───────────────────────────────────────────────────

$Services = Get-Service |
    Where-Object { $_.Status -eq 'Running' -and $_.StartType -ne 'Disabled' } |
    Sort-Object -Property DisplayName |
    Select-Object -First 10 -Property Name, DisplayName, @{
        Name       = 'Uptime'
        Expression = { (Get-Date) - $_.StartTime }
    }

$Numbers  = 1..20
$EvenSum  = ($Numbers | Where-Object { $_ % 2 -eq 0 } | Measure-Object -Sum).Sum
$Squares  = $Numbers.ForEach({ $_ * $_ })

# ── String features ──────────────────────────────────────────────────────────

$HereString = @"
Application : $AppName
Start Time  : $($StartTime.ToString('yyyy-MM-dd HH:mm'))
Config Keys : $($Config.Keys -join ', ')
"@

$LiteralHere = @'
No $variables expanded here.
Backslashes \ and quotes " are kept as-is.
'@

$Escaped  = "Line one`nLine two`tTabbed"
$Composed = "App: $AppName | Retries: $MaxRetries | Dry: $DryRun"

# ── Control flow ─────────────────────────────────────────────────────────────

switch -Regex ($Config.LogLevel) {
    '^D'      { $LevelValue = 0 }
    '^I'      { $LevelValue = 1 }
    '^W'      { $LevelValue = 2 }
    '^E'      { $LevelValue = 3 }
    default   { $LevelValue = 1 }
}

$Rating = switch ($true) {
    ($Threshold -ge 0.9) { 'Excellent' }
    ($Threshold -ge 0.7) { 'Good' }
    default              { 'Needs improvement' }
}

foreach ($tag in $Config.Tags) {
    $slug = $tag | ConvertTo-Slug
    Write-Output "Tag: $tag -> $slug"
}

$Label = if ($DryRun) { 'SIMULATION' } else { 'LIVE' }

# ── Error handling and scopes ────────────────────────────────────────────────

try {
    $result = Invoke-WithRetry -ScriptBlock {
        $response = Invoke-RestMethod -Uri 'https://api.example.com/health' -TimeoutSec 5
        if ($response.status -ne 'ok') {
            throw "Unexpected status: $($response.status)"
        }
        $response
    } -RetryCount 2
}
catch {
    Write-Error "Unhandled error: $($_.Exception.GetType().Name) - $_"
}
finally {
    $elapsed = (Get-Date) - $StartTime
    Write-Output "Completed in $($elapsed.TotalSeconds)s"
}

# ── Splatting and advanced calls ─────────────────────────────────────────────

$CopyParams = @{
    Path        = 'C:\Source\data.csv'
    Destination = $Config.OutputPath
    Force       = $true
    ErrorAction = 'SilentlyContinue'
}

if (-not $DryRun) {
    Copy-Item @CopyParams
}

# ── Comparison and logical operators ─────────────────────────────────────────

$IsValid   = ($AppName -like 'Ful*') -and ($MaxRetries -ge 1)
$HasMatch  = 'hello-world' -match '^[a-z]+(-[a-z]+)*$'
$Replaced  = 'foo_bar_baz' -replace '_', '-'
$InList    = 'powershell' -in $Config.Tags
$Joined    = $Config.Tags -join ' | '

# ── Pipeline chains and cmdlet calls ─────────────────────────────────────────

Test-Connection -ComputerName 'localhost' -Count 1 -Quiet && Write-Output 'Reachable' || Write-Warning 'Unreachable'

Get-ChildItem -Path '.' -Filter '*.ps1' -Recurse |
    ForEach-Object { Get-FormattedSize $_.Length }

$Report = @(
    "Name:      $AppName"
    "Threshold: $Threshold"
    "Rating:    $Rating"
    "Valid:     $IsValid"
)
$Report | ForEach-Object { Write-Output $_ }
