<#
.SYNOPSIS
    Compile Mode exe differential — exe 출력이 `un run`과 바이트 동일한지 검증한다.
.DESCRIPTION
    실행: powershell -File scripts\exe-diff.ps1
    - 파트 A (embed): test/compile/*.un 전부를 `un build --embed`로 링크한 exe를 실행,
      `un run`과 stdout/stderr/exit code를 바이트 비교한다.
      (이 fixture들은 test.assert 같은 미등록 모듈을 import 하므로 plain `un build`로는
      못 빌드하지만, --embed는 임의 프로그램을 지원한다.)
    - 파트 B (plain): test/compile/build/*.un 을 plain `un build`로 링크한 exe가
      `un run`과 stdout/stderr/exit code 바이트 동일한지 검사한다. 이 디렉터리는 plain
      native exe로 컴파일 가능한 프로그램만 두므로 자동으로 exe 검증이 된다.
    - 파트 D (negative): test/compile/build_negative/manifest.txt 의 각 행에 대해
      plain `un build`가 exit 1 + 기대 문자열을 내는지 검사한다. 지원하지 않는 구문이
      조용히 무시되거나 잘못 컴파일되지 않는지를 고정하는 게이트다.
    종료 코드: 하나라도 [FAIL]이면 1, 아니면 0.
#>
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$Exe = Join-Path $Root "rust\target\release\un.exe"
if (-not (Test-Path $Exe)) {
    Write-Host "missing executable: $Exe" -ForegroundColor Red
    exit 1
}
$Tmp = Join-Path $env:TEMP ("exe_diff_" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $Tmp -Force | Out-Null

function Invoke-Raw {
    # Spawns a process with System.Diagnostics.Process and returns raw stdout/
    # stderr bytes plus the exit code (no console re-encoding anywhere).
    param(
        [string]$FilePath,
        [string[]]$ArgumentList,
        [string]$Tag,
        [string]$WorkDir,
        [int]$TimeoutMs = 60000
    )
    $argStr = ($ArgumentList | ForEach-Object {
        if ($_ -match '\s') { '"' + $_ + '"' } else { $_ }
    }) -join ' '
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $FilePath
    $psi.Arguments = $argStr
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.WorkingDirectory = $WorkDir
    $p = New-Object System.Diagnostics.Process
    $p.StartInfo = $psi
    if (-not $p.Start()) {
        return @{ code = -1; out = @(); err = @(); timeout = $false }
    }
    if (-not $p.WaitForExit($TimeoutMs)) {
        try { $p.Kill() } catch {}
        return @{ code = -1; out = @(); err = @(); timeout = $true }
    }
    $outMs = New-Object System.IO.MemoryStream
    $errMs = New-Object System.IO.MemoryStream
    # Tiny outputs only; re-reading after exit avoids the 4KB pipe deadlock.
    $p.StandardOutput.BaseStream.CopyTo($outMs)
    $p.StandardError.BaseStream.CopyTo($errMs)
    return @{ code = $p.ExitCode; out = $outMs.ToArray(); err = $errMs.ToArray(); timeout = $false }
}

function Bytes-Equal([byte[]]$a, [byte[]]$b) {
    if ($a.Length -ne $b.Length) { return $false }
    return [System.Linq.Enumerable]::SequenceEqual([byte[]]$a, [byte[]]$b)
}

$pass = 0; $fail = 0
$results = @()

function Check-Case {
    param([string]$Label, [string]$Tag, [string[]]$BuildArgs, [string[]]$RunArgs)
    $run = Invoke-Raw -FilePath $Exe -ArgumentList $RunArgs -Tag "${Tag}_run" -WorkDir $Root
    $build = Invoke-Raw -FilePath $Exe -ArgumentList $BuildArgs -Tag "${Tag}_build" -WorkDir $Root
    if ($build.code -ne 0) {
        Write-Host "[FAIL] $Label (build exit $($build.code))" -ForegroundColor Red
        $script:fail += 1
        return
    }
    $exePath = $BuildArgs[-1]
    $exe = Invoke-Raw -FilePath $exePath -ArgumentList @() -Tag "${Tag}_exe" -WorkDir $Root
    $okOut = Bytes-Equal $run.out $exe.out
    $okErr = Bytes-Equal $run.err $exe.err
    if ($okOut -and $okErr -and ($run.code -eq $exe.code)) {
        Write-Host "[PASS] $Label" -ForegroundColor Green
        $script:pass += 1
    } else {
        $why = @()
        if (-not $okOut) { $why += "stdout differs" }
        if (-not $okErr) { $why += "stderr differs" }
        if ($run.code -ne $exe.code) { $why += "exit code $($run.code) != $($exe.code)" }
        Write-Host "[FAIL] $Label ($($why -join ', '))" -ForegroundColor Red
        $script:fail += 1
    }
}

# Part A: --embed (any-program VM in exe) must match `un run` byte-for-byte.
$partA = Get-ChildItem (Join-Path $Root "test\compile") -Filter *.un -File | Sort-Object Name
foreach ($f in $partA) {
    $exePath = Join-Path $Tmp ($f.BaseName + ".exe")
    Check-Case -Label "embed/$($f.Name)" -Tag "embed_$($f.BaseName)" `
        -BuildArgs @("build", "--embed", "--allow-dynamic", $f.FullName, "-o", $exePath) `
        -RunArgs @("run", $f.FullName)
}

# Part B: plain native exe for opt-in buildable fixtures.
$partBDir = Join-Path $Root "test\compile\build"
if (Test-Path $partBDir) {
    $partB = Get-ChildItem $partBDir -Filter *.un -File | Sort-Object Name
    foreach ($f in $partB) {
        $exePath = Join-Path $Tmp ("plain_" + $f.BaseName + ".exe")
        Check-Case -Label "build/$($f.Name)" -Tag "plain_$($f.BaseName)" `
            -BuildArgs @("build", $f.FullName, "-o", $exePath) `
            -RunArgs @("run", $f.FullName)
    }
}

# Part C: a plain build that needs no VM bridge must emit the no-VM marker
# and an exe far smaller than the libunvm.a-linked fallback (~17 MB).
$nrtFixture = Join-Path $Root "test\compile\build\bare_for.un"
if (Test-Path $nrtFixture) {
    $exePath = Join-Path $Tmp "nrt_bare_for.exe"
    $run = Invoke-Raw -FilePath $Exe -ArgumentList @("run", $nrtFixture) -Tag "nrt_run" -WorkDir $Root
    $build = Invoke-Raw -FilePath $Exe -ArgumentList @("build", $nrtFixture, "-o", $exePath) -Tag "nrt_build" -WorkDir $Root
    $marker = [Text.Encoding]::UTF8.GetString($build.out)
    if ($build.code -ne 0 -or $marker -notmatch 'no-VM native runtime') {
        Write-Host "[FAIL] nrt/bare_for.un (no-VM marker missing)" -ForegroundColor Red
        $script:fail += 1
    } elseif (-not (Test-Path $exePath) -or (Get-Item $exePath).Length -gt 10MB) {
        Write-Host "[FAIL] nrt/bare_for.un (exe too large — VM likely linked)" -ForegroundColor Red
        $script:fail += 1
    } else {
        # Do not name this `$exe`: PowerShell variables are case-insensitive and
        # that would clobber the `$Exe` path used by every part of this script.
        $nrtExe = Invoke-Raw -FilePath $exePath -ArgumentList @() -Tag "nrt_exe" -WorkDir $Root
        if ((Bytes-Equal $run.out $nrtExe.out) -and (Bytes-Equal $run.err $nrtExe.err) -and ($run.code -eq $nrtExe.code)) {
            Write-Host "[PASS] nrt/bare_for.un (no VM linked)" -ForegroundColor Green
            $script:pass += 1
        } else {
            Write-Host "[FAIL] nrt/bare_for.un (output differs)" -ForegroundColor Red
            $script:fail += 1
        }
    }
}

# Part D: plain builds must reject unsupported programs with an explicit
# diagnostic (exit 1 + expected substring) instead of silently dropping or
# miscompiling them. Manifest rows are "path|expected substring"; paths are
# relative to test/compile/build_negative. Blank/comment rows are ignored.
$partDDir = Join-Path $Root "test\compile\build_negative"
$partDManifest = Join-Path $partDDir "manifest.txt"
if (Test-Path $partDManifest) {
    foreach ($line in (Get-Content $partDManifest)) {
        $entry = $line.Trim()
        if ($entry -eq "" -or $entry.StartsWith("#")) { continue }
        $parts = $entry.Split("|", 2)
        $name = $parts[0].Trim()
        if ($parts.Length -ne 2 -or $name -eq "") {
            Write-Host "[FAIL] neg/manifest (malformed row: $entry)" -ForegroundColor Red
            $script:fail += 1
            continue
        }
        $needle = $parts[1].Trim()
        $label = "neg/$name"
        $src = Join-Path $partDDir $name
        if (-not (Test-Path $src)) {
            Write-Host "[FAIL] $label (missing fixture)" -ForegroundColor Red
            $script:fail += 1
            continue
        }
        $exePath = Join-Path $Tmp ("neg_" + [guid]::NewGuid().ToString("N") + ".exe")
        $r = Invoke-Raw -FilePath $Exe -ArgumentList @("build", $src, "-o", $exePath) -Tag "neg" -WorkDir $Root
        $text = [Text.Encoding]::UTF8.GetString($r.out) + [Text.Encoding]::UTF8.GetString($r.err)
        if ($r.code -eq 1 -and $text.Contains($needle)) {
            Write-Host "[PASS] $label" -ForegroundColor Green
            $script:pass += 1
        } else {
            $why = @()
            if ($r.code -ne 1) { $why += "exit $($r.code) != 1" }
            if (-not $text.Contains($needle)) { $why += "missing '$needle'" }
            Write-Host "[FAIL] $label ($($why -join ', '))" -ForegroundColor Red
            $script:fail += 1
        }
    }
}

Remove-Item $Tmp -Recurse -Force -ErrorAction SilentlyContinue
Write-Host ""
Write-Host "exe-diff: $pass passed, $fail failed"
if ($fail -gt 0) { exit 1 }
