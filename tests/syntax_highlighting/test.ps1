# ── Module and strict mode ───────────────────────────────────────────────────

#Requires -Version 7.0
#Requires -Modules Az.Storage, Az.KeyVault
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

using namespace System.Collections.Generic
using namespace System.IO

# ── Variables and types ──────────────────────────────────────────────────────

[string]$AppName      = 'Fulgur'
[int]$MaxRetries      = 3
[double]$Threshold    = 0.85
[bool]$DryRun         = $false
[datetime]$StartTime  = Get-Date
[hashtable]$Config    = @{
    LogLevel   = 'Info'
    OutputPath = Join-Path $env:USERPROFILE '.fulgur'
    Tags       = @('syntax', 'highlight', 'powershell')
    Nested     = @{ Enabled = $true; Depth = 5 }
}

# ── Enums and classes ────────────────────────────────────────────────────────

enum Severity {
    Debug
    Info
    Warning
    Error
}

class LogEntry {
    [datetime]$Timestamp
    [Severity]$Level
    [string]$Message

    LogEntry([Severity]$level, [string]$message) {
        $this.Timestamp = Get-Date
        $this.Level     = $level
        $this.Message   = $message
    }

    [string] ToString() {
        return "[$($this.Level)] $($this.Timestamp.ToString('HH:mm:ss')) - $($this.Message)"
    }
}

# ── Functions ────────────────────────────────────────────────────────────────

function Invoke-WithRetry {
    [CmdletBinding()]
    [OutputType([object])]
    param(
        [Parameter(Mandatory)]
        [scriptblock]$ScriptBlock,

        [ValidateRange(1, 10)]
        [int]$RetryCount = $MaxRetries,

        [int]$DelayMilliseconds = 200
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

$Escaped = "Line one`nLine two`tTabbed"

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

# ── Error handling and scopes ────────────────────────────────────────────────

try {
    $result = Invoke-WithRetry -ScriptBlock {
        $response = Invoke-RestMethod -Uri 'https://api.example.com/health' -TimeoutSec 5
        if ($response.status -ne 'ok') {
            throw [InvalidOperationException]::new("Unexpected status: $($response.status)")
        }
        $response
    } -RetryCount 2
}
catch [System.Net.Http.HttpRequestException] {
    Write-Error "Network error: $_"
}
catch {
    Write-Error "Unhandled error: $($_.Exception.GetType().Name) - $_"
}
finally {
    $elapsed = (Get-Date) - $StartTime
    [LogEntry]::new([Severity]::Info, "Completed in $($elapsed.TotalSeconds)s") |
        ForEach-Object { $_.ToString() } |
        Write-Output
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

# ── Ternary, null-coalescing, and pipeline chain ─────────────────────────────

$Label    = $DryRun ? 'SIMULATION' : 'LIVE'
$FallBack = $Config['MissingKey'] ?? 'default-value'

Test-Connection -ComputerName 'localhost' -Count 1 -Quiet && Write-Output 'Reachable' || Write-Warning 'Unreachable'
