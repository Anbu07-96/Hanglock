<#
.SYNOPSIS
    One command from a clean checkout to a runnable (and optionally packaged) build.
#>
[CmdletBinding()]
param(
    [ValidateSet("x64", "arm64", "both")][string]$Arch = "x64",
    [switch]$Installer,
    [switch]$Test,
    [string]$Version = "0.1.0"
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
Set-Location $root

$targets = switch ($Arch) {
    "x64"   { @("x86_64-pc-windows-msvc") }
    "arm64" { @("aarch64-pc-windows-msvc") }
    "both"  { @("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc") }
}
foreach ($t in $targets) {
    Write-Host "== $t =="
    cargo build --release --target $t -p hanglock
    if ($Test) { cargo test --workspace --target $t }
    $exe = Join-Path $root "target\$t\release\hanglock.exe"
    $kb = [math]::Ceiling((Get-Item $exe).Length / 1KB)
    Write-Host ("  {0}: {1} KB" -f $exe, $kb)
    if ($kb -gt 2048) { throw "executable $kb KB exceeds the 2048 KB budget (docs/architecture.md §8)" }
}
if ($Installer) {
    $iscc = @(
        "$env:ProgramFiles(x86)\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles}\Inno Setup 6\ISCC.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $iscc) { throw "Inno Setup 6 not found; install it or pass -Installer:`$false" }
    & $iscc "/DAppVersion=$Version" "/DBuildDir=..\target\x86_64-pc-windows-msvc\release" (Join-Path $root "scripts\installer.iss")
}
