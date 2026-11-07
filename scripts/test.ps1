<#
.SYNOPSIS
    Un 테스트 러너. C:\dev\un 에서 실행한다.

.DESCRIPTION
    test/MANIFEST.txt에 등록된 스크립트를 순서대로 실행한다.
    각 스크립트는 test/assert.un의 assert.summary()를 호출하므로
    "=== Result: N passed, M failed ===" 줄을 집계해 총합을 출력한다.

    - test/negative/manifest.txt : 비정상 입력/구문 오류 스크립트.
      형식: <path>|<expected exit code>|<expected message substring>
    - test/KnownFailures.txt : 현재 실패가Known된 스크립트. 집계에 포함하지 않는다.

.PARAMETER Group
    all(기본) | unit | integration | regression | negative | known | suite

.PARAMETER File
    특정 스크립트 하나만 실행 (MANIFEST 우회).

.PARAMETER Rebuild
    dotnet build -c Release를 먼저 수행한다.

.EXAMPLE
    powershell -File scripts/test.ps1
    powershell -File scripts/test.ps1 -Group unit
    powershell -File scripts/test.ps1 -File test/unit/native/str.un
#>
param(
    [string]$Group = "all",
    [string]$File = "",
    [switch]$Rebuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$env:DOTNET_ROLL_FORWARD = "Major"
$Exe = Join-Path $Root "bin\Release\net8.0\Un.exe"

if ($Rebuild) {
    dotnet build -c Release | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "build failed" -ForegroundColor Red
        exit 1
    }
}

if (-not (Test-Path $Exe)) {
    Write-Host "missing executable: $Exe (use -Rebuild)" -ForegroundColor Red
    exit 1
}

function Invoke-Un([string]$path) {
    $prev = $ErrorActionPreference
    # Un은 잡히지 않는 예외를 stderr에 쓴다. Stop을 유지하면 PowerShell이
    # stderr를 예외로 바꿔 러너가 중단된다. 이 함수만 Continue로 실행한다.
    $ErrorActionPreference = "Continue"
    $out = & $Exe run $path 2>&1
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    return @{ code = $code; text = ($out -join "`n") }
}

function Parse-Result([string]$text) {
    $m = [regex]::Match($text, "=== Result: (\d+) passed, (\d+) failed ===")
    if ($m.Success) {
        return @{ passed = [int]$m.Groups[1].Value; failed = [int]$m.Groups[2].Value }
    }
    return $null
}

function Run-Positive([string[]]$files) {
    $sumPassed = 0
    $sumFailed = 0
    $crashed = 0
    $badFiles = 0
    $results = @()

    foreach ($f in $files) {
        $r = Invoke-Un $f
        $res = Parse-Result $r.text
        if ($null -eq $res) {
            $crashed += 1
            $badFiles += 1
            Write-Host ("[CRASH] {0} (exit {1})" -f $f, $r.code) -ForegroundColor Red
            ($r.text -split "`n") | ForEach-Object { "    " + $_ }
            continue
        }

        $sumPassed += $res.passed
        $sumFailed += $res.failed

        if ($res.failed -eq 0 -and $r.code -eq 0) {
            Write-Host ("[PASS]  {0} ({1} assertions)" -f $f, $res.passed) -ForegroundColor Green
        }
        else {
            $badFiles += 1
            Write-Host ("[FAIL]  {0} ({1} passed, {2} failed, exit {3})" -f $f, $res.passed, $res.failed, $r.code) -ForegroundColor Red
            ($r.text -split "`n") | Where-Object { $_ -like "[FAIL]*" } | ForEach-Object { "    " + $_ }
        }
    }

    return @{ passed = $sumPassed; failed = $sumFailed; crashed = $crashed; files = $files.Count; badFiles = $badFiles; results = $results }
}

function Run-Negative() {
    $manifest = Join-Path $Root "test\negative\manifest.txt"
    if (-not (Test-Path $manifest)) {
        Write-Host "negative manifest missing" -ForegroundColor Red
        return @{ matched = 0; mismatched = 0 }
    }

    $matched = 0
    $mismatched = 0

    foreach ($line in (Get-Content $manifest)) {
        $line = $line.Trim()
        if ($line -eq "" -or $line.StartsWith("#")) { continue }

        $parts = $line -split "\|"
        $path = $parts[0]
        $wantCode = [int]$parts[1]
        $wantMsg = $parts[2]

        $r = Invoke-Un (Join-Path $Root $path)

        if ($r.code -eq $wantCode -and $r.text -like "*$wantMsg*") {
            Write-Host ("[NEG OK]  {0} (exit {1})" -f $path, $r.code) -ForegroundColor Green
            $matched += 1
        }
        else {
            $mismatched += 1
            Write-Host ("[NEG BAD] {0} (exit {1}, expected {2} / '{3}')" -f $path, $r.code, $wantCode, $wantMsg) -ForegroundColor Red
            ($r.text -split "`n") | Select-Object -First 4 | ForEach-Object { "    " + $_ }
        }
    }

    return @{ matched = $matched; mismatched = $mismatched }
}

function Run-Known() {
    $manifest = Join-Path $Root "test\KnownFailures.txt"
    if (-not (Test-Path $manifest)) { return @{ failing = 0; fixed = 0 } }

    $failing = 0
    $fixed = 0

    foreach ($line in (Get-Content $manifest)) {
        $line = $line.Trim()
        if ($line -eq "" -or $line.StartsWith("#")) { continue }

        $r = Invoke-Un (Join-Path $Root $line)
        $res = Parse-Result $r.text

        if ($null -ne $res -and $res.failed -gt 0) {
            Write-Host ("[KNOWN FAIL] {0} ({1} failing)" -f $line, $res.failed) -ForegroundColor Yellow
            $failing += 1
        }
        else {
            Write-Host ("[KNOWN FIXED] {0} -> now passing" -f $line) -ForegroundColor Yellow
            $fixed += 1
        }
    }

    return @{ failing = $failing; fixed = $fixed }
}

# MANIFEST에서 실행할 그룹을 고른다.
$manifestPath = Join-Path $Root "test\MANIFEST.txt"
$groups = @()
foreach ($line in (Get-Content $manifestPath)) {
    $line = $line.Trim()
    if ($line -eq "" -or $line.StartsWith("#")) { continue }
    $parts = $line.Split("|")
    $groups += [pscustomobject]@{ name = $parts[0]; path = $parts[1] }
}

if ($File -ne "") {
    $r = Invoke-Un $File
    Write-Host ("exit={0}" -f $r.code)
    Write-Host $r.text
    exit 0
}

$selected = $groups | Where-Object {
    if ($Group -eq "all") { $_.name -ne "known" -and $_.name -ne "negative" }
    elseif ($Group -eq "suite") { $_.name -eq "suite" }
    else { $_.name -eq $Group }
}

Write-Host ("Un test runner - group: {0}" -f $Group)
Write-Host ("executable: {0}" -f $Exe)

if ($selected.Count -gt 0) {
    $pos = Run-Positive ($selected | ForEach-Object { $_.path })
    Write-Host ""
    Write-Host ("Positive: {0} files, {1} passed, {2} failed, {3} crashed" -f $pos.files, $pos.passed, $pos.failed, $pos.crashed)
}

$neg = $null
$known = $null

if ($Group -eq "all" -or $Group -eq "negative") {
    Write-Host ""
    $neg = Run-Negative
    Write-Host ("Negative: {0} matched, {1} mismatched" -f $neg.matched, $neg.mismatched)
}

if ($Group -eq "all" -or $Group -eq "known") {
    Write-Host ""
    $known = Run-Known
    Write-Host ("Known failures: {0} still failing, {1} unexpectedly passing" -f $known.failing, $known.fixed)
}

$exit = 0
if ($Group -eq "all" -or $Group -eq "unit" -or $Group -eq "integration" -or $Group -eq "regression" -or $Group -eq "suite") {
    if ($pos.failed -gt 0 -or $pos.crashed -gt 0) { $exit = 1 }
}
if ($neg -and $neg.mismatched -gt 0) { $exit = 1 }
if ($known -and $known.fixed -gt 0) { $exit = 1 }

Write-Host ""
Write-Host ("Overall exit code: {0}" -f $exit)
exit $exit
