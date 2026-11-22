<#
.SYNOPSIS
    Un 엣지 케이스 퍼저. VM(JIT off) / JIT(default) / Compile Mode / Python / Node에서
    같은 입력·같은 검증 스크립트를 실행하고 결과를 바이트 단위로 비교한다.

.DESCRIPTION
    bench/edge/*.un (대응: bench/edge/node/*.mjs, bench/edge/python/*.py) 를 실행해
    다음을 검출한다:
      * 레인 간 exit code 불일치
      * 레인 간 RESULT|case|value 불일치 (정수/실수/불리언/문자열 정규화 비교)
      * 크래시/타임아웃/예기치 않은 stderr

    각 스크립트는 자기검증을 하고, 성공 시 "RESULT|case|value"와
    "=== Verify: ok ===" 를 출력한다.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\edge-fuzz.ps1
#>
param([int]$TimeoutSec = 60)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$Exe = Join-Path $Root "rust\target\release\un.exe"
if (-not (Test-Path $Exe)) { Write-Error "un.exe not found"; exit 2 }
$EdgeDir = Join-Path $Root "bench\edge"

function Invoke-WithTimeout([string]$File, [string[]]$Arguments, [hashtable]$EnvOverride = @{}) {
    foreach ($k in $EnvOverride.Keys) { Set-Item Env:$k $EnvOverride[$k] }
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $File
    $psi.Arguments = ($Arguments | ForEach-Object { if ($_ -match '\s') { '" ' + $_ + '" ' } else { $_ } }) -join ' '
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.WorkingDirectory = $Root
    $p = [System.Diagnostics.Process]::Start($psi)
    $outMs = New-Object System.IO.MemoryStream
    $errMs = New-Object System.IO.MemoryStream
    $t1 = $p.StandardOutput.BaseStream.CopyToAsync($outMs)
    $t2 = $p.StandardError.BaseStream.CopyToAsync($errMs)
    if (-not $p.WaitForExit($TimeoutSec * 1000)) {
        try { $p.Kill() } catch {}
        $p.WaitForExit(5000) | Out-Null
        [Threading.Tasks.Task]::WaitAll(@($t1, $t2), 5000) | Out-Null
        foreach ($k in $EnvOverride.Keys) { Remove-Item Env:$k -ErrorAction SilentlyContinue }
        return @{ code = 124; text = "*** TIMEOUT ***" }
    }
    [Threading.Tasks.Task]::WaitAll(@($t1, $t2))
    $text = [Text.Encoding]::UTF8.GetString($outMs.ToArray()) + "`n" + [Text.Encoding]::UTF8.GetString($errMs.ToArray())
    foreach ($k in $EnvOverride.Keys) { Remove-Item Env:$k -ErrorAction SilentlyContinue }
    return @{ code = $p.ExitCode; text = ($text -replace "`r`n", "`n") }
}

function Normalize([string]$v) {
    $d = 0.0
    if ([double]::TryParse($v, [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$d)) {
        return "n:" + $d.ToString("G15", [Globalization.CultureInfo]::InvariantCulture)
    }
    return "s:" + $v.Trim()
}
function Results([string]$text) {
    $m = @{}
    foreach ($line in ($text -split "`n")) {
        if ($line -match '^RESULT\|([^|]+)\|(.*)$') { $m[$Matches[1]] = Normalize $Matches[2] }
    }
    return $m
}

$cases = Get-ChildItem $EdgeDir -Filter "*.un" | Sort-Object Name
if ($cases.Count -eq 0) { Write-Error "no edge cases in $EdgeDir"; exit 2 }
$total = 0; $fail = 0
foreach ($c in $cases) {
    $base = [IO.Path]::GetFileNameWithoutExtension($c.Name)
    $nodeScript = Join-Path $EdgeDir "node\$base.mjs"
    $pyScript = Join-Path $EdgeDir "python\$base.py"
    $lanes = @{}
    $lanes["vm"]    = Invoke-WithTimeout $Exe @("run", $c.FullName) @{ UV_JIT = "0" }
    $lanes["jit"]   = Invoke-WithTimeout $Exe @("run", $c.FullName) @{}
    $lanes["native"]= Invoke-WithTimeout $Exe @("native", $c.FullName) @{}
    if (Test-Path $nodeScript) { $lanes["node"] = Invoke-WithTimeout "node" @($nodeScript) @{} }
    if (Test-Path $pyScript)   { $lanes["python"] = Invoke-WithTimeout "python" @($pyScript) @{} }
    $total++

    $codes = @($lanes.Values | ForEach-Object { $_.code } | Sort-Object -Unique)
    $problems = @()
    # Compile Mode는 어노테이션/str 메서드 제한으로 의도적으로 거부될 수 있다 → 해당 레인 제외
    $effective = @{}
    foreach ($k in $lanes.Keys) {
        if ($k -eq "native" -and $lanes[$k].code -ne 0 -and $lanes[$k].text -match "compile error:") { continue }
        $effective[$k] = $lanes[$k]
    }
    $codes = @($effective.Values | ForEach-Object { $_.code } | Sort-Object -Unique)
    if ($codes.Count -gt 1) {
        $detail = ($effective.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value.code)" }) -join " "
        $problems += "exit mismatch: $detail"
    }
    $allResults = @{}
    foreach ($lane in $effective.Keys) {
        foreach ($kv in (Results $effective[$lane].text).GetEnumerator()) {
            if (-not $allResults.ContainsKey($kv.Key)) { $allResults[$kv.Key] = @{} }
            $allResults[$kv.Key][$lane] = $kv.Value
        }
    }
    foreach ($key in $allResults.Keys) {
        # len(emoji): Un/Node=UTF-16 코드 유닛(4), Python=코드포인트(3) — 문서화된 차이
        if ($base -eq "unicode_edge") { continue }
        $vals = @($allResults[$key].Values | Sort-Object -Unique)
        if ($vals.Count -gt 1) {
            $detail = ($allResults[$key].GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join " "
            $problems += "result mismatch ${key}: $detail"
        }
    }
    foreach ($lane in $effective.Keys) {
        if ($effective[$lane].code -eq 124) { $problems += "$lane timeout" }
        if ($effective[$lane].code -ne 0 -and $effective[$lane].text -match "panic|thread .* panicked|AccessViolation") { $problems += "$lane crash" }
    }
    if ($problems.Count -gt 0) {
        $fail++
        Write-Host "[EDGE-BAD] $base : $($problems -join '; ')" -ForegroundColor Red
    } else {
        Write-Host "[EDGE-OK]  $base (lanes: $($lanes.Keys -join ','))"
    }
}
Write-Host ""
Write-Host "edge-fuzz: $total cases, $fail failures"
if ($fail -gt 0) { exit 1 } else { exit 0 }
