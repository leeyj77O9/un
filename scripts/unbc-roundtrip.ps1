<#
.SYNOPSIS
    .unbc 왕복(round-trip) 검증 러너. C:\dev\un 에서 실행한다.

.DESCRIPTION
    Positive 매니페스트(test/MANIFEST.txt, known/negative 제외)에 등록된
    스크립트마다 다음 순서로 실행하고 결과를 바이트 단위로 비교한다.

      1. un run <file>              → stdout/stderr 바이트, 종료 코드
      2. un compile <file>          → <file>.unbc 생성
      3. un run <file>.unbc         → stdout/stderr 바이트, 종료 코드

    1과 3이 다르면 FAIL이다. Positive 파일이 컴파일되지 않아도 FAIL이다.
    생성된 <file>.unbc 는 비교 직후 삭제한다(.gitignore에도 *.unbc 등록됨).

    -IncludeNegative : test/negative/manifest.txt 의 파일도 왕복 비교한다.
    파싱 단계에서 거부되는 negative 파일은 바이트코드를 만들 수 없으므로
    "skipped" 로 집계한다(컴파일 실패 자체는 negative 에 정상).

.PARAMETER Group
    all(기본) | unit | integration | regression | suite  (test.ps1과 같은 선택 규칙)

.PARAMETER File
    특정 스크립트 하나만 왕복 검증 (MANIFEST 조회).

.PARAMETER Rebuild
    cargo build --release를 먼저 실행한다.

.EXAMPLE
    powershell -File scripts/unbc-roundtrip.ps1
    powershell -File scripts/unbc-roundtrip.ps1 -File test/test_suite.un
    powershell -File scripts/unbc-roundtrip.ps1 -IncludeNegative
#>
param(
    [string]$Group = "all",
    [string]$File = "",
    [switch]$IncludeNegative,
    [switch]$Rebuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$rustup = Join-Path $env:USERPROFILE ".cargo\bin\rustup.exe"
$cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
$Exe = Join-Path $Root "rust\target\release\un.exe"
$Toolchain = "stable-x86_64-pc-windows-gnu"
$Manifest = Join-Path $Root "rust\Cargo.toml"

if ($Rebuild) {
    & $rustup run $Toolchain $cargo build --release --manifest-path $Manifest
    if ($LASTEXITCODE -ne 0) {
        Write-Host "build failed" -ForegroundColor Red
        exit 1
    }
}

if (-not (Test-Path $Exe)) {
    Write-Host "missing executable: $Exe (use -Rebuild)" -ForegroundColor Red
    exit 1
}

# .NET Process로 stdout/stderr를 바이트로 캡처한다. 두 스트림은 비동기로
# 펌프하므로 파이프 버퍼가 차서 생기는 교착은 없다. 인코딩과 무관하게
# 비교할 수 있다.
function Invoke-Bytes([string]$arguments, [int]$TimeoutMs = 120000) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe
    $psi.Arguments = $arguments
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.WorkingDirectory = $Root
    $p = [System.Diagnostics.Process]::Start($psi)
    $outMs = New-Object System.IO.MemoryStream
    $errMs = New-Object System.IO.MemoryStream
    $tOut = $p.StandardOutput.BaseStream.CopyToAsync($outMs)
    $tErr = $p.StandardError.BaseStream.CopyToAsync($errMs)
    if (-not $p.WaitForExit($TimeoutMs)) {
        # Hang: kill and report a synthetic timeout code.
        try { $p.Kill() } catch {}
        $p.WaitForExit(5000) | Out-Null
        [Threading.Tasks.Task]::WaitAll(@($tOut, $tErr), 5000) | Out-Null
        return @{
            code   = 124
            stdout = $outMs.ToArray()
            stderr = $errMs.ToArray()
        }
    }
    [Threading.Tasks.Task]::WaitAll(@($tOut, $tErr))
    return @{
        code   = $p.ExitCode
        stdout = $outMs.ToArray()
        stderr = $errMs.ToArray()
    }
}

function Bytes-Equal([byte[]]$a, [byte[]]$b) {
    if ($null -eq $a -or $null -eq $b) { return $false }
    if ($a.Length -ne $b.Length) { return $false }
    return [System.Linq.Enumerable]::SequenceEqual($a, $b)
}

function Preview([byte[]]$bytes, [int]$max = 300) {
    if ($null -eq $bytes -or $bytes.Length -eq 0) { return "" }
    $text = [System.Text.Encoding]::UTF8.GetString($bytes)
    if ($text.Length -gt $max) { return $text.Substring(0, $max) + "..." }
    return $text
}

function RoundTrip([string]$path) {
    $direct = Invoke-Bytes ('run "' + $path + '"')
    $compile = Invoke-Bytes ('compile "' + $path + '"')
    $unbcPath = $path + ".unbc"
    $round = $null
    try {
        if ($compile.code -eq 0 -and (Test-Path $unbcPath)) {
            $round = Invoke-Bytes ('run "' + $unbcPath + '"')
        }
    } finally {
        Remove-Item $unbcPath -Force -ErrorAction SilentlyContinue
    }
    if ($null -eq $round) {
        return @{ ok = $false; reason = ("compile failed (exit {0})" -f $compile.code) }
    }
    $sameOut = Bytes-Equal $direct.stdout $round.stdout
    $sameErr = Bytes-Equal $direct.stderr $round.stderr
    $sameCode = ($direct.code -eq $round.code)
    if ($sameOut -and $sameErr -and $sameCode) {
        return @{ ok = $true; reason = "" }
    }
    $why = @()
    if (-not $sameCode) { $why += "exit {0} -> {1}" -f $direct.code, $round.code }
    if (-not $sameOut) { $why += "stdout differs (direct: {0} | unbc: {1})" -f (Preview $direct.stdout), (Preview $round.stdout) }
    if (-not $sameErr) { $why += "stderr differs (direct: {0} | unbc: {1})" -f (Preview $direct.stderr), (Preview $round.stderr) }
    return @{ ok = $false; reason = ($why -join "; ") }
}

function Parse-Manifest([string]$path) {
    $rows = @()
    foreach ($line in (Get-Content $path)) {
        $line = $line.Trim()
        if ($line -eq "" -or $line.StartsWith("#")) { continue }
        $parts = $line.Split("|")
        $rows += [pscustomobject]@{ group = $parts[0].Trim(); path = $parts[1].Trim() }
    }
    return $rows
}

if ($File -ne "") {
    $r = RoundTrip $File
    if ($r.ok) {
        Write-Host ("ROUNDTRIP OK  {0}" -f $File)
        exit 0
    }
    Write-Host ("ROUNDTRIP FAIL {0}: {1}" -f $File, $r.reason) -ForegroundColor Red
    exit 1
}

$manifestPath = Join-Path $Root "test\MANIFEST.txt"
$rows = Parse-Manifest $manifestPath
$selected = $rows | Where-Object {
    if ($Group -eq "all") { $_.group -ne "known" -and $_.group -ne "negative" }
    else { $_.group -eq $Group }
}

$ok = 0
$fail = 0
$failed = @()
foreach ($entry in $selected) {
    $r = RoundTrip $entry.path
    if ($r.ok) {
        $ok++
        Write-Host ("ok   {0}" -f $entry.path)
    } else {
        $fail++
        $failed += [pscustomobject]@{ path = $entry.path; reason = $r.reason }
        Write-Host ("FAIL {0}: {1}" -f $entry.path, $r.reason) -ForegroundColor Red
    }
}
Write-Host ""
Write-Host ("RoundTrip(positive): {0} files, {1} identical, {2} mismatched" -f ($ok + $fail), $ok, $fail)

if ($IncludeNegative) {
    $negPath = Join-Path $Root "test\negative\manifest.txt"
    $negOk = 0
    $negFail = 0
    $negSkip = 0
    foreach ($line in (Get-Content $negPath)) {
        $line = $line.Trim()
        if ($line -eq "" -or $line.StartsWith("#")) { continue }
        $parts = $line.Split("|")
        $path = $parts[0].Trim()
        $r = RoundTrip $path
        if ($r.ok) {
            $negOk++
        } elseif ($r.reason -like "compile failed*") {
            $negSkip++
        } else {
            $negFail++
            Write-Host ("FAIL(negative) {0}: {1}" -f $path, $r.reason) -ForegroundColor Red
        }
    }
    Write-Host ("RoundTrip(negative): {0} identical, {1} mismatched, {2} skipped(compile-rejected)" -f $negOk, $negFail, $negSkip)
    $fail += $negFail
}

foreach ($f in $failed) {
    Write-Host ("failed: {0}" -f $f.path)
}
if ($fail -gt 0) { exit 1 }
exit 0
