<#
.SYNOPSIS
    Un differential runner. C:\dev\un 에서 실행한다.

.DESCRIPTION
    Rust VM(rust\target\release\unvm.exe)과 C# reference(bin\Release\net8.0\Un.exe)를
    같은 입력으로 실행해 stdout/exit code(및 negative의 stderr substring)를 비교한다.

    -Group:
      positive  : test/MANIFEST.txt의 suite/unit/integration/known 스크립트
                  (stdout byte-identical + exit code 일치)
      negative  : test/negative/manifest.txt의 path|expected exit|expected substring
                  (substring이 비어 있으면 exit code만 검증). negative 4개는 문서화된
                  차이이므로 KNOWN-DIFF로 보고하고 실패로 세지 않는다
                  (order_crash: .NET Array.Sort inconsistent results /
                   depth_limit_crash, property_recursion: stack overflow exit code /
                   go_loop_var: go scope racy)
      regex     : test/probes/regex/probe_re*.un (stdout byte-identical + exit code)
      help      : `unvm help` vs `Un.exe help` (stdout byte-identical + exit 0)
      all       : 전부(기본값)

    종료 코드: 예상 못한 DIFF가 있으면 1, 없으면 0.

.PARAMETER Group
    positive | negative | regex | help | all (기본)

.PARAMETER Rebuild
    cargo build --release를 먼저 수행한다.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/differential.ps1
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/differential.ps1 -Rebuild
#>
param(
    [string]$Group = "all",
    [switch]$Rebuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$env:DOTNET_ROLL_FORWARD = "Major"
$Ref = Join-Path $Root "bin\Release\net8.0\Un.exe"
$Rust = Join-Path $Root "rust\target\release\unvm.exe"

if ($Rebuild) {
    & rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path (Join-Path $Root "rust\Cargo.toml")
    if ($LASTEXITCODE -ne 0) { exit 1 }
}

foreach ($exe in @($Ref, $Rust)) {
    if (-not (Test-Path $exe)) {
        Write-Host "missing: $exe" -ForegroundColor Red
        if ($exe -eq $Ref) { Write-Host "reference: rebuild the C# project first (dotnet build -c Release)" -ForegroundColor Red }
        if ($exe -eq $Rust) { Write-Host "rust: pass -Rebuild to build it first" -ForegroundColor Red }
        exit 1
    }
}

$failures = 0
$known = 0
$checked = 0
$refTmp = Join-Path $env:TEMP "undiff_ref"
$rustTmp = Join-Path $env:TEMP "undiff_rust"

function Run-Un([string]$exe, [string]$file, [string]$prefix) {
    # the reference runner merges `2>&1` into one stream; substrings are matched against the merged text
    cmd /c "`"$exe`" run `"$file`" > `"$prefix.out`" 2>&1"
    $code = $LASTEXITCODE
    return @(
        [System.IO.File]::ReadAllText("$prefix.out"),
        $code
    )
}

function Report([string]$status, [string]$label) {
    if ($status -eq "PASS") {
        Write-Host "PASS $label" -ForegroundColor Green
    } elseif ($status -eq "KNOWN") {
        Write-Host "KNOWN-DIFF $label (문서화된 차이)" -ForegroundColor Yellow
        $script:known++
    } else {
        Write-Host "DIFF $label" -ForegroundColor Red
        $script:failures++
    }
    $script:checked++
}

function Test-Positive([string]$rel) {
    $file = Join-Path $Root $rel
    $r = Run-Un $Ref $file $refTmp
    $u = Run-Un $Rust $file $rustTmp
    $status = if ($r[0] -ceq $u[0] -and $r[1] -eq $u[1]) { "PASS" } else { "DIFF" }
    Report $status $rel
}

if ($Group -eq "all" -or $Group -eq "positive") {
    Write-Host "== positive =="
    $lines = Get-Content (Join-Path $Root "test\MANIFEST.txt")
    foreach ($line in $lines) {
        if ($line -match '^\s*#') { continue }
        if ($line -notmatch '^(suite|unit|integration|known)\|') { continue }
        $rel = ($line -split '\|', 2)[1].Trim()
        Test-Positive $rel
    }
}

if ($Group -eq "all" -or $Group -eq "negative") {
    Write-Host "== negative =="
    # 문서화된 차이(4개)는 KNOWN-DIFF로 기록하고 실패로 세지 않는다
    $knownDivergent = @(
        "test/negative/order_crash.un",
        "test/negative/depth_limit_crash.un",
        "test/negative/property_recursion.un",
        "test/negative/go_loop_var.un"
    )
    $lines = Get-Content (Join-Path $Root "test\negative\manifest.txt")
    foreach ($line in $lines) {
        if ($line -match '^\s*#') { continue }
        if ($line.Trim() -eq '') { continue }
        $parts = $line.Split('|', 3)
        if ($parts.Count -lt 3) { continue }
        $rel = $parts[0].Trim()
        if ($rel -eq 'path') { continue }
        $wantExit = [int]$parts[1].Trim()
        $wantMatch = $parts[2].Trim()
        $file = Join-Path $Root $rel

        $r = Run-Un $Ref $file $refTmp
        $u = Run-Un $Rust $file $rustTmp

        $refOk = ($r[1] -eq $wantExit) -and ($wantMatch -eq '' -or $r[0].Contains($wantMatch))
        $rustOk = ($u[1] -eq $wantExit) -and ($wantMatch -eq '' -or $u[0].Contains($wantMatch))

        if ($refOk -and $rustOk) {
            Report "PASS" $rel
        } elseif ($knownDivergent -contains $rel) {
            Report "KNOWN" "$rel (want exit $wantExit sub '$wantMatch'; ref exit $($r[1]) / rust exit $($u[1]))"
        } else {
            Report "DIFF" "$rel (want exit $wantExit sub '$wantMatch'; ref exit $($r[1]) / rust exit $($u[1]))"
        }
    }
}

if ($Group -eq "all" -or $Group -eq "regex") {
    Write-Host "== regex probes =="
    Get-ChildItem (Join-Path $Root "test\probes\regex") -Filter "probe_re*.un" | ForEach-Object {
        $r = Run-Un $Ref $_.FullName $refTmp
        $u = Run-Un $Rust $_.FullName $rustTmp
        $status = if ($r[0] -ceq $u[0] -and $r[1] -eq $u[1]) { "PASS" } else { "DIFF" }
        Report $status $_.Name
    }
}

if ($Group -eq "all" -or $Group -eq "help") {
    Write-Host "== help =="
    cmd /c "`"$Ref`" help > `"$refTmp.out`" 2>&1"
    $refCode = $LASTEXITCODE
    $refHelp = [System.IO.File]::ReadAllText("$refTmp.out")
    cmd /c "`"$Rust`" help > `"$rustTmp.out`" 2>&1"
    $rustCode = $LASTEXITCODE
    $rustHelp = [System.IO.File]::ReadAllText("$rustTmp.out")
    $status = if ($refHelp -ceq $rustHelp -and $refCode -eq $rustCode) { "PASS" } else { "DIFF" }
    Report $status "help"
}

Write-Host ""
Write-Host "differential: $checked checked, $failures unexpected DIFF, $known known-divergent"
if ($failures -gt 0) { exit 1 }
exit 0
