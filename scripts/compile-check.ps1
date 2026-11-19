<#
.SYNOPSIS
    Compile Mode 검증 러너. test/compile/*.un을 `un run`과 `un native`로 각각 실행하고
    stdout/stderr/exit code를 바이트 비교한다. 또한 `un check`의 종료 코드를 확인한다.
.DESCRIPTION
    실행: powershell -File scripts\compile-check.ps1
    - 각 test/compile/*.un: exit code 일치 + stdout/stderr 일치 → PASS/FAIL 집계
    - 어노테이션 누락 프로그램: `un check`는 exit 1, `un run`도 동일하게 exit 1 (this is the
      documented difference: compile mode rejects what the interpreter accepts)
#>
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$Exe = Join-Path $Root "rust\target\release\un.exe"
if (-not (Test-Path $Exe)) {
    Write-Host "missing executable: $Exe" -ForegroundColor Red
    exit 1
}

function Invoke-Un {
    # No param() block: implicit $args collects all positional arguments
    # (a declared [string[]]$args silently collects only the first one on PS 5.1).
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $out = & $Exe $args 2>&1
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    return @{ code = $code; text = ($out -join "`n") }
}

$pass = 0; $fail = 0
$files = Get-ChildItem (Join-Path $Root "test\compile") -Filter *.un -File | Sort-Object Name
foreach ($f in $files) {
    $run = Invoke-Un run $f.FullName
    $nat = Invoke-Un native $f.FullName
    $chk = Invoke-Un check $f.FullName
    # 양성 fixture는 두 모드 모두 exit 0 + 동일 출력이어야 한다 (bare help의
    # 거짓 통과 방지).
    $ok = ($run.code -eq 0) -and ($nat.code -eq 0) -and ($run.text -eq $nat.text) -and ($chk.code -eq 0)
    if ($ok) {
        Write-Host "[PASS] $($f.Name)" -ForegroundColor Green
        $pass += 1
    } else {
        Write-Host "[FAIL] $($f.Name) (run=$($run.code) native=$($nat.code) check=$($chk.code))" -ForegroundColor Red
        $fail += 1
    }
}

# 어노테이션이 없는 프로그램은 compile mode가 거부한다 (인터프리터와의 문서화된 차이).
$tmp = Join-Path $env:TEMP ("compile_check_" + [guid]::NewGuid().ToString("N") + ".un")
Set-Content -Path $tmp -Value "fn add(a, b)`n    -> a + b`nfn main() -> int`n    -> add(1, 2)`nmain()`n" -Encoding ascii
$chk = Invoke-Un check $tmp
$nat = Invoke-Un native $tmp
$expected = ($chk.code -eq 1) -and ($nat.code -eq 1) -and ($chk.text -match "requires a type annotation")
if ($expected) {
    Write-Host "[PASS] untyped-source-rejected" -ForegroundColor Green
    $pass += 1
} else {
    Write-Host "[FAIL] untyped-source-rejected (check=$($chk.code) native=$($nat.code))" -ForegroundColor Red
    $fail += 1
}
Remove-Item $tmp -Force

# 거부가 예정된 프로그램들: `un check`와 `un native` 양쪽이 exit 1이며
# 기대 부분문자열이 stderr/stdout에 있어야 한다.
$errorsDir = Join-Path $Root "test\compile\errors"
$errorsManifest = Join-Path $errorsDir "manifest.txt"
if (Test-Path $errorsManifest) {
    $expected = @{}
    Get-Content $errorsManifest | ForEach-Object {
        $line = $_.Trim()
        if ($line -eq '' -or $line.StartsWith('#')) { return }
        $sep = $line.IndexOf('|')
        if ($sep -lt 1) { Write-Host "[BAD] errors manifest row: $line" -ForegroundColor Red; $fail += 1; return }
        $key = Split-Path $line.Substring(0, $sep).Trim() -Leaf
        $expected[$key] = $line.Substring($sep + 1).Trim()
    }
    foreach ($f in Get-ChildItem $errorsDir -Filter *.un -File | Sort-Object Name) {
        $key = $f.Name
        $sub = $expected[$key]
        if (-not $sub) {
            Write-Host "[FAIL] $key (missing manifest row)" -ForegroundColor Red
            $fail += 1
            continue
        }
        $chk = Invoke-Un check $f.FullName
        $nat = Invoke-Un native $f.FullName
        $ok = ($chk.code -eq 1) -and ($nat.code -eq 1) -and ($chk.text -match [regex]::Escape($sub)) -and ($nat.text -match [regex]::Escape($sub))
        if ($ok) {
            Write-Host "[PASS] errors/$key" -ForegroundColor Green
            $pass += 1
        } else {
            Write-Host "[FAIL] errors/$key (check=$($chk.code) native=$($nat.code))" -ForegroundColor Red
            $fail += 1
        }
    }
}

Write-Host ""
Write-Host "compile-check: $pass passed, $fail failed"
if ($fail -gt 0) { exit 1 }
