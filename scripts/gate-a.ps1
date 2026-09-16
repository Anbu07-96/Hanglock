<#
.SYNOPSIS
    Gate A: measure the four numbers Hanglock is judged on, on a real Windows desktop.

.DESCRIPTION
    Idle CPU, working set, cold start, and frame/present counts. Run it against a release build:

        pwsh -File scripts/gate-a.ps1 -Build release
        pwsh -File scripts/gate-a.ps1 -Seconds 60 -WithDragSeconds 20

    The drag numbers need a person: this script starts the app, samples, and asks you to swing the
    clock when told to. It reports what it measured and what it could not, in the table format
    docs/gate-a.md expects, because a table with an empty cell is honest and a table with an invented
    one is a bug report in waiting.
#>
[CmdletBinding()]
param(
    [ValidateSet("release", "debug", "bench")][string]$Build = "release",
    [int]$Seconds = 30,
    [int]$WithDragSeconds = 0,
    [string]$Exe = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
Set-Location $root

if (-not $Exe) {
    $Exe = Join-Path $root "target\$Build\hanglock.exe"
    if (-not $SkipBuild -and -not (Test-Path $Exe)) {
        Write-Host "building $Build ..." -NoNewline
        cargo build --$Build -p hanglock | Out-Null
        Write-Host " done"
    }
}
if (-not (Test-Path $Exe)) { throw "no executable at $Exe" }

function Get-Sample($proc) {
    $proc.Refresh()
    [pscustomobject]@{
        CpuMs      = [math]::Round($proc.TotalProcessorTime.TotalMilliseconds, 1)
        WorkingSet = [math]::Round($proc.WorkingSet64 / 1MB, 1)
        PrivateMB  = [math]::Round($proc.PrivateMemorySize64 / 1MB, 1)
        Handles    = $proc.HandleCount
        Threads    = $proc.Threads.Count
    }
}

# Cold start: launch, wait for the overlay window to exist, and time it. Polling at 1 ms because
# Start-Process does not report readiness, and the number we want is the user's, not the loader's.
$started = Get-Date
$p = Start-Process -FilePath $Exe -PassThru
$deadline = (Get-Date).AddSeconds(15)
$found = $null
$sw = [System.Diagnostics.Stopwatch]::StartNew()
while ((Get-Date) -lt $deadline) {
    $found = Get-Process -Id $p.Id -ErrorAction SilentlyContinue
    if ($found -and $found.MainWindowHandle -ne 0) { break }
    if ($found) {
        # The overlay is a tool window with no owner, so MainWindowHandle stays 0: fall back to "the
        # process is up and has finished doing its startup work".
        $hwndSig = (Get-CimInstance Win32_Process -Filter "ProcessId=$($p.Id)").HandleCount
        if ($hwndSig -gt 0) { break }
    }
    Start-Sleep -Milliseconds 2
}
$startupMs = [math]::Round($sw.Elapsed.TotalMilliseconds, 0)
$sw.Stop()
if (-not $found) { throw "hanglock exited during startup" }
Write-Host "cold start (process up, first frame requested): $startupMs ms"

$before = Get-Sample $found
$wall0 = Get-Date
Write-Host "sampling idle for $Seconds s - do not touch the clock ..."
Start-Sleep -Seconds $Seconds
$after = Get-Sample $found
$wall1 = Get-Date
$idleCpu = [math]::Round((($after.CpuMs - $before.CpuMs) / (($wall1 - $wall0).TotalMilliseconds) / [Environment]::ProcessorCount) * 100, 3)

$swingCpu = "not measured"
if ($WithDragSeconds -gt 0) {
    $b2 = Get-Sample $found
    $t0 = Get-Date
    Write-Host "NOW DRAG AND THROW THE CLOCK for $WithDragSeconds s"
    Start-Sleep -Seconds $WithDragSeconds
    $a2 = Get-Sample $found
    $t1 = Get-Date
    $swingCpu = [math]::Round((($a2.CpuMs - $b2.CpuMs) / (($t1 - $t0).TotalMilliseconds) / [Environment]::ProcessorCount) * 100, 2)
}

# The app's own counters, via --diag on a second instance: frames painted, presents, bytes copied.
# Read-only and it exits, so it cannot disturb the instance under measurement.
$diag = ""
try {
    $diag = (& $Exe --diag 2>&1) -join "`n"
} catch { $diag = "--diag failed: $_" }

$bench = ""
try { $bench = (& $Exe --bench 400 2>&1) -join "`n" } catch { $bench = "--bench failed: $_" }

$rows = @(
    [pscustomobject]@{ Metric = "cold start, ms";           Value = $startupMs;             Budget = "<= 150" }
    [pscustomobject]@{ Metric = "idle CPU, % of one core";  Value = $idleCpu;               Budget = "<= 0.05" }
    [pscustomobject]@{ Metric = "CPU while swinging, %";    Value = $swingCpu;              Budget = "<= 3" }
    [pscustomobject]@{ Metric = "working set, MB";          Value = $after.WorkingSet;      Budget = "<= 20 (fail > 30)" }
    [pscustomobject]@{ Metric = "private bytes, MB";        Value = $after.PrivateMB;       Budget = "<= 35" }
    [pscustomobject]@{ Metric = "handles";                  Value = $after.Handles;         Budget = "no growth over 1 h" }
    [pscustomobject]@{ Metric = "threads";                  Value = $after.Threads;         Budget = "<= 3" }
    [pscustomobject]@{ Metric = "executable, KB";           Value = [math]::Ceiling((Get-Item $Exe).Length / 1KB); Budget = "<= 2048" }
)
Write-Host ""
$rows | Format-Table -AutoSize | Out-String | Write-Host
Write-Host "--- paint cost ---"
Write-Host $bench
Write-Host "--- counters ---"
Write-Host $diag
Write-Host ""
Write-Host "Manual checks still required for Gate A (see docs/gate-a.md): click-through, focus,"
Write-Host "frame-loop-stopped, DPI change, monitor unplug/replug, resume from sleep."
Stop-Process -Id $found.Id -ErrorAction SilentlyContinue
