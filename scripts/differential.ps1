<#
.SYNOPSIS
    Differential runner: executes every test script twice — once with the JIT
    disabled (UV_JIT=0) and once with the default configuration — and requires
    identical results.

.DESCRIPTION
    The register VM and the .unbc JIT must agree byte-for-byte on observable
    output and exit code. This script runs every file in test/MANIFEST.txt
    (suite/unit/integration/regression groups) and in test/negative/manifest.txt
    under both configurations and fails the run on any difference:

    * Positive scripts: stdout must be identical.
    * Negative scripts: exit code must be identical (the substring expectation
      is configuration-independent by construction — it is a language error
      message, not an engine error message).

.PARAMETER TimeoutSec
    Per-run timeout in seconds (default 120). A hung script fails the run instead
    of blocking it indefinitely.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\differential.ps1
#>

param(
    [int]$TimeoutSec = 120
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$Exe = Join-Path $Root "rust\target\release\unvm.exe"
if (-not (Test-Path $Exe)) {
    Write-Error "unvm.exe not found at $Exe — build it first: rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust/Cargo.toml"
    exit 2
}

function Invoke-Un([string]$Path, [string]$JitMode) {
    # "0" pins the JIT off; "" pins nothing (default configuration).
    if ($JitMode -eq "") {
        Remove-Item Env:UV_JIT -ErrorAction SilentlyContinue
    } else {
        $env:UV_JIT = $JitMode
    }
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe
    $psi.Arguments = "run `"$Path`""
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.WorkingDirectory = $Root
    $p = [System.Diagnostics.Process]::Start($psi)
    $outMs = New-Object System.IO.MemoryStream
    $errMs = New-Object System.IO.MemoryStream
    $tOut = $p.StandardOutput.BaseStream.CopyToAsync($outMs)
    $tErr = $p.StandardError.BaseStream.CopyToAsync($errMs)
    if (-not $p.WaitForExit($TimeoutSec * 1000)) {
        try { $p.Kill() } catch {}
        $p.WaitForExit(5000) | Out-Null
        [Threading.Tasks.Task]::WaitAll(@($tOut, $tErr), 5000) | Out-Null
        return @{ code = 124; text = "*** TIMEOUT after ${TimeoutSec}s ***" }
    }
    [Threading.Tasks.Task]::WaitAll(@($tOut, $tErr))
    $stdout = [System.Text.Encoding]::UTF8.GetString($outMs.ToArray())
    $stderr = [System.Text.Encoding]::UTF8.GetString($errMs.ToArray())
    $text = (($stdout, $stderr) -join "`n") -replace "`r`n", "`n" -replace "(`n){3,}", "`n"
    return @{ code = $p.ExitCode; text = $text }
}

function Compare-One([string]$Path) {
    $off = Invoke-Un $Path "0"
    $on  = Invoke-Un $Path ""
    if ($off.code -ne $on.code) {
        return "[JIT-DIFF] $Path (UV_JIT=0 exit $($off.code) vs default exit $($on.code))"
    }
    if ($off.text -ne $on.text) {
        $diffLine = -1
        $a = $off.text -split "`n"
        $b = $on.text -split "`n"
        for ($i = 0; $i -lt $a.Count -or $i -lt $b.Count; $i++) {
            $sa = $null; $sb = $null
            if ($i -lt $a.Count) { $sa = $a[$i] }
            if ($i -lt $b.Count) { $sb = $b[$i] }
            if ($sa -ne $sb) { $diffLine = $i + 1; break }
        }
        return "[JIT-DIFF] $Path (line $diffLine differs)"
    }
    return ""
}

$total = 0
$diffs = 0

# ---------- positive manifest ----------
$manifestPath = Join-Path $Root "test\MANIFEST.txt"
foreach ($line in (Get-Content $manifestPath)) {
    $line = $line.Trim()
    if ($line -eq "" -or $line.StartsWith("#")) { continue }
    $parts = $line.Split("|", 2)
    $group = $parts[0]
    $path = $parts[1]
    if ($group -notin @("suite", "unit", "integration", "regression", "known")) { continue }
    $total++
    $d = Compare-One $path
    if ($d -ne "") {
        Write-Host $d
        $diffs++
    }
    else {
        Write-Host ("[JIT-OK]   {0}" -f $path)
    }
}

# ---------- negative manifest ----------
$negativePath = Join-Path $Root "test\negative\manifest.txt"
foreach ($line in (Get-Content $negativePath)) {
    $line = $line.Trim()
    if ($line -eq "" -or $line.StartsWith("#")) { continue }
    $parts = $line.Split("|", 3)
    $path = $parts[0]
    $total++
    $d = Compare-One $path
    if ($d -ne "") {
        Write-Host $d
        $diffs++
    }
    else {
        Write-Host ("[JIT-OK]   {0}" -f $path)
    }
}

Write-Host ""
Write-Host ("Differential: {0} files, {1} differences" -f $total, $diffs)
if ($diffs -gt 0) { exit 1 } else { exit 0 }
