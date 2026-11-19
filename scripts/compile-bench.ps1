<#
.SYNOPSIS
    Compile Mode 벤치마크. `bench/native_fib.un`을 VM/JIT/native 세 모드로 측정하고 비교한다.
.DESCRIPTION
    실행: powershell -File scripts\compile-bench.ps1 [-Rebuild] [-NoSave]
    결과는 표준 출력에 표로 나오고, -NoSave가 아니면 bench\results\<stamp>-compile\에 CSV로 저장된다.
    각 모드의 검증(`=== Result: 2 passed, 0 failed ===`)도 함께 확인한다.
#>
param(
    [switch]$Rebuild,
    [switch]$NoSave
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$Exe = Join-Path $Root "rust\target\release\un.exe"
$Toolchain = "stable-x86_64-pc-windows-gnu"
$Manifest = Join-Path $Root "rust\Cargo.toml"
$Stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$ResultsDir = Join-Path $Root "bench\results\$Stamp-compile"
$Culture = [System.Globalization.CultureInfo]::InvariantCulture

if ($Rebuild) {
    $rustup = Join-Path $env:USERPROFILE ".cargo\bin\rustup.exe"
    $cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    & $rustup run $Toolchain $cargo build --release --manifest-path $Manifest
    if ($LASTEXITCODE -ne 0) { exit 1 }
}
if (-not (Test-Path $Exe)) {
    Write-Host "missing executable: $Exe (use -Rebuild)" -ForegroundColor Red
    exit 1
}

function Invoke-Mode([string]$label, [string]$jitEnv) {
    # No param() block for the executable arguments: implicit $args collects
    # them all (a declared [string[]]$args silently collects only the first).
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $saved = $env:UV_JIT
    if ($null -eq $jitEnv) { Remove-Item Env:UV_JIT -ErrorAction SilentlyContinue } else { $env:UV_JIT = $jitEnv }
    $out = & $Exe $args 2>&1
    $code = $LASTEXITCODE
    $env:UV_JIT = $saved
    if ($null -eq $saved) { Remove-Item Env:UV_JIT -ErrorAction SilentlyContinue }
    $ErrorActionPreference = $prev
    $median = $null
    foreach ($line in ($out -split "`n")) {
        if ($line -match 'BENCH\|native_fib24\|1\|7\|2\|([\d.]+)\|([\d.]+)') {
            $median = [double]$Matches[2]
        }
    }
    $passed = "?"
    foreach ($line in ($out -split "`n")) {
        if ($line -match '=== Result: (\d+) passed, (\d+) failed ===') { $passed = "$($Matches[1])/$($Matches[2])"; break }
    }
    return [pscustomobject]@{ label = $label; code = $code; median = $median; passed = $passed }
}

$rows = @()
$rows += Invoke-Mode "un run (VM+JIT)" $null run bench/native_fib.un
$rows += Invoke-Mode "un run (UV_JIT=0)" "0" run bench/native_fib.un
$rows += Invoke-Mode "un native" $null native bench/native_fib.un

Write-Host ""
Write-Host ("{0,-22} {1,6} {2,12} {3,10}" -f "mode", "exit", "median(ms)", "asserts")
foreach ($r in $rows) {
    $median = if ($null -ne $r.median) { $r.median.ToString("0.000", $Culture) } else { "n/a" }
    Write-Host ("{0,-22} {1,6} {2,12} {3,10}" -f $r.label, $r.code, $median, $r.passed)
}

$bad = $rows | Where-Object { $_.code -ne 0 -or $_.passed -ne "2/0" }
if ($bad) {
    Write-Host ""
    Write-Host "compile-bench: FAILED (nonzero exit or failed assertions)" -ForegroundColor Red
    exit 1
}

if (-not $NoSave) {
    New-Item -ItemType Directory -Path $ResultsDir -Force | Out-Null
    $csv = Join-Path $ResultsDir "native_fib24.csv"
    $lines = @("mode,exit,median_ms,asserts")
    foreach ($r in $rows) { $lines += ("{0},{1},{2},{3}" -f $r.label, $r.code, $r.median, $r.passed) }
    Set-Content -Path $csv -Value $lines -Encoding ascii
    Write-Host ""
    Write-Host "saved: $csv"
}
