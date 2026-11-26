<#
.SYNOPSIS
    `un build --parallel` opt-in gate — go/wait parallel cases only.

.DESCRIPTION
    Runs powershell -File scripts\gowait-parallel.ps1 from the repository root.

    This gate is deliberately independent of the eight byte-identical default
    gates (test.ps1, differential.ps1, unbc-roundtrip.ps1, compile-check.ps1,
    exe-diff.ps1, edge-fuzz.ps1, strict canaries, cargo test). It never calls
    them and never changes their expected counts.

    For every row of test/parallel/manifest.txt it either
      * mode=run    : builds `un build --parallel <fixture> -o <unique path>`,
                      runs the produced plain exe and checks
                        - exit code (strict),
                        - the RESULT|case|value multiset (sorted, order-free),
                        - the error text in stdout+stderr (strict substring),
                      or
      * mode=reject : runs `un build --embed --parallel <fixture>` and checks
                      the build-time exit 2 refusal plus its diagnostic text.

    Cache influence is excluded both ways: UV_BUILD_CACHE=0 is set for every
    child process and every build writes to a fresh per-invocation temp dir.

    Task start order, wall-clock time and direct task prints are NOT compared;
    only the normalized RESULT| multiset is.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\gowait-parallel.ps1
#>
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$Exe = Join-Path $Root "rust\target\release\un.exe"
if (-not (Test-Path $Exe)) {
    Write-Host "missing executable: $Exe (build with cargo build --release first)" -ForegroundColor Red
    exit 2
}

$Manifest = Join-Path $Root "test\parallel\manifest.txt"
if (-not (Test-Path $Manifest)) {
    Write-Host "missing manifest: $Manifest" -ForegroundColor Red
    exit 2
}

# Raw process invocation (no PowerShell native-command error wrapping), with
# a hard timeout so a deadlocked scheduler fails the gate instead of hanging.
function Invoke-Proc {
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$WorkDir,
        [int]$TimeoutMs = 60000
    )
    $argStr = ($Arguments | ForEach-Object {
        if ($_ -match '\s') { '"' + $_ + '"' } else { $_ }
    }) -join ' '
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $FilePath
    $psi.Arguments = $argStr
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.WorkingDirectory = $WorkDir
    $p = New-Object System.Diagnostics.Process
    $p.StartInfo = $psi
    if (-not $p.Start()) {
        return @{ code = -1; out = ""; err = ""; timeout = $false }
    }
    $outMs = New-Object System.IO.MemoryStream
    $errMs = New-Object System.IO.MemoryStream
    $tOut = $p.StandardOutput.BaseStream.CopyToAsync($outMs)
    $tErr = $p.StandardError.BaseStream.CopyToAsync($errMs)
    if (-not $p.WaitForExit($TimeoutMs)) {
        try { $p.Kill() } catch {}
        $p.WaitForExit(5000) | Out-Null
        [Threading.Tasks.Task]::WaitAll(@($tOut, $tErr), 5000) | Out-Null
        return @{ code = 124; out = ""; err = ""; timeout = $true }
    }
    [Threading.Tasks.Task]::WaitAll(@($tOut, $tErr))
    return @{
        code    = $p.ExitCode
        out     = [Text.Encoding]::UTF8.GetString($outMs.ToArray())
        err     = [Text.Encoding]::UTF8.GetString($errMs.ToArray())
        timeout = $false
    }
}

# RESULT|case|value -> sorted "case=value" multiset. Sorting (not hashing)
# keeps duplicates, so a lost or duplicated task result is still detected.
function Get-Results([string]$text) {
    $pairs = @()
    foreach ($line in (($text -replace "`r`n", "`n") -split "`n")) {
        if ($line -match '^RESULT\|([^|]+)\|(.*)$') {
            $pairs += ($Matches[1].Trim() + "=" + $Matches[2].Trim())
        }
    }
    return @($pairs | Sort-Object)
}

function Expected-Results([string]$field) {
    if ($field -eq "-") { return @() }
    return @($field.Split(";") | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" } | Sort-Object)
}

$prevCache = $env:UV_BUILD_CACHE
$env:UV_BUILD_CACHE = "0"
$Tmp = Join-Path $env:TEMP ("gowait_parallel_" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $Tmp -Force | Out-Null

$pass = 0
$fail = 0
$cases = 0

try {
    foreach ($line in (Get-Content $Manifest)) {
        $entry = $line.Trim()
        if ($entry -eq "" -or $entry.StartsWith("#")) { continue }
        $parts = $entry.Split("|")
        if ($parts.Length -ne 5) {
            Write-Host "[FAIL] manifest row (expected 5 fields): $entry" -ForegroundColor Red
            $fail++
            continue
        }
        $path = $parts[0].Trim()
        $wantExit = [int]$parts[1].Trim()
        $wantResults = Expected-Results $parts[2].Trim()
        $wantError = $parts[3].Trim()
        $mode = $parts[4].Trim()
        $src = Join-Path $Root ("test\parallel\" + $path)
        if (-not (Test-Path $src)) {
            Write-Host "[FAIL] $path (missing fixture)" -ForegroundColor Red
            $fail++
            continue
        }
        $cases++
        $label = "$mode/$path"
        $exePath = Join-Path $Tmp ($path.Replace("\", "_").Replace("/", "_").Replace(".un", "") + "_" + $cases + ".exe")

        if ($mode -eq "reject") {
            $r = Invoke-Proc -FilePath $Exe `
                -Arguments @("build", "--embed", "--parallel", "test\parallel\$path", "-o", $exePath) `
                -WorkDir $Root -TimeoutMs 120000
            $problems = @()
            if ($r.timeout) { $problems += "build timeout" }
            if ($r.code -ne $wantExit) { $problems += "build exit $($r.code) != $wantExit" }
            if ($wantError -ne "-" -and -not ($r.out + "`n" + $r.err).Contains($wantError)) {
                $problems += "missing '$wantError'"
            }
            if (Test-Path $exePath) { $problems += "rejected build still produced an exe" }
            if ($problems.Count -eq 0) {
                Write-Host "[PASS] $label" -ForegroundColor Green
                $pass++
            } else {
                Write-Host "[FAIL] $label ($($problems -join '; '))" -ForegroundColor Red
                $fail++
            }
            continue
        }

        if ($mode -ne "run") {
            Write-Host "[FAIL] $label (unknown mode '$mode')" -ForegroundColor Red
            $fail++
            continue
        }

        $build = Invoke-Proc -FilePath $Exe `
            -Arguments @("build", "--parallel", "test\parallel\$path", "-o", $exePath) `
            -WorkDir $Root -TimeoutMs 120000
        if ($build.timeout) {
            Write-Host "[FAIL] $label (build timeout)" -ForegroundColor Red
            $fail++
            continue
        }
        if ($build.code -ne 0 -or -not (Test-Path $exePath)) {
            $msg = ($build.out + "`n" + $build.err).Trim() -replace "`n", " "
            Write-Host "[FAIL] $label (build exit $($build.code), no exe: $msg)" -ForegroundColor Red
            $fail++
            continue
        }

        $r = Invoke-Proc -FilePath $exePath -Arguments @() -WorkDir $Root -TimeoutMs 30000
        $problems = @()
        if ($r.timeout) {
            $problems += "run timeout (unresolved task / deadlock)"
        } else {
            if ($r.code -ne $wantExit) { $problems += "exit $($r.code) != $wantExit" }
            $actual = Get-Results $r.out
            if (($actual -join "`t") -ne ($wantResults -join "`t")) {
                $problems += "RESULT mismatch: got [$($actual -join ', ')] expected [$($wantResults -join ', ')]"
            }
            $combined = $r.out + "`n" + $r.err
            if ($wantError -eq "-") {
                if ($r.err.Trim() -ne "") { $problems += "unexpected stderr: $($r.err.Trim())" }
                if ($combined -match '(?m)^error:') { $problems += "unexpected error text" }
            } elseif (-not $combined.Contains($wantError)) {
                $problems += "missing '$wantError'"
            }
        }
        if ($problems.Count -eq 0) {
            Write-Host "[PASS] $label ($($wantResults.Count) results)" -ForegroundColor Green
            $pass++
        } else {
            Write-Host "[FAIL] $label ($($problems -join '; '))" -ForegroundColor Red
            $fail++
        }
    }
}
finally {
    if ($null -ne $prevCache) { $env:UV_BUILD_CACHE = $prevCache }
    else { Remove-Item Env:UV_BUILD_CACHE -ErrorAction SilentlyContinue }
    Remove-Item $Tmp -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "gowait-parallel: $pass passed, $fail failed ($cases cases)"
if ($fail -gt 0) { exit 1 }
exit 0
