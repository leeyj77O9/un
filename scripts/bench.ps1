<#
.SYNOPSIS
    Un 벤치마크 러너. C:\dev\un 에서 실행한다.

.DESCRIPTION
    bench/*.un 과 bench/node/*.mjs 을 같은 순서로 실행하고, 같은 label의 결과를 짝지어 비교한다.

    측정 규칙(두 러너 모두 동일):
      - warmup번 준비 실행 후 reps번 실행을 하나씩 시간 재고 Min/Median/Average/Max/StdDev를 낸다.
      - Median은 정렬된 값의 [n/2]이다. OPS는 Median 기준 iters/ms다.
      - 시간 측정 구간에는 출력이 포함되지 않는다.
      - 각 스크립트는 측정 전에 검증(assert)하고 그 결과를 출력한다.

    Un과 Node는 같은 알고리즘/입력/기대값을 사용한다(각 스크립트 상단 주석과 검증 문).
    Un에 메모리 측정 API가 없어서 heap 열은 Node의 힙 증가분만 기록된다.

.PARAMETER Group
    all(기본) | un | node

.PARAMETER Rebuild
    cargo build --release를 먼저 수행한다.

.PARAMETER NoSave
    bench/results에 저장하지 않는다.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group un
#>
param(
    [string]$Group = "all",
    [switch]$Rebuild,
    [switch]$NoSave
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$rustup = Join-Path $env:USERPROFILE ".cargo\bin\rustup.exe"
$cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
$Exe = Join-Path $Root "rust\target\release\unvm.exe"
$Toolchain = "stable-x86_64-pc-windows-gnu"
$Manifest = Join-Path $Root "rust\Cargo.toml"
$Culture = [System.Globalization.CultureInfo]::InvariantCulture

function Fmt([double]$v, [string]$format) {
    return ([double]$v).ToString($format, $Culture)
}

function Parse-BenchLine([string]$line) {
    $i = $line.IndexOf("BENCH|")
    if ($i -lt 0) { return $null }
    $parts = $line.Substring($i).Split("|")
    if ($parts.Count -lt 11) { return $null }
    return @{
        label  = $parts[1]
        iters  = [int]$parts[2]
        reps   = [int]$parts[3]
        warmup = [int]$parts[4]
        min    = [double]$parts[5]
        median = [double]$parts[6]
        avg    = [double]$parts[7]
        max    = [double]$parts[8]
        stddev = [double]$parts[9]
        ops    = [double]$parts[10]
    }
}

function Parse-MemLine([string]$line) {
    $i = $line.IndexOf("MEM|")
    if ($i -lt 0) { return $null }
    $parts = $line.Substring($i).Split("|")
    if ($parts.Count -lt 4) { return $null }
    return @{ label = $parts[1]; delta = [double]$parts[2]; heap = [double]$parts[3] }
}

function Get-Validation([string]$text) {
    # Un은 assert.summary(), Node는 "=== Verify: ok ==="를 출력한다.
    foreach ($line in $text -split "`n") {
        if ($line -match "=== Result: (\d+) passed, (\d+) failed ===") {
            return "{0} passed, {1} failed" -f $Matches[1], $Matches[2]
        }
        if ($line -match "=== Verify: ok ===") {
            return "verify ok"
        }
    }
    return "none"
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

function Invoke-Node([string]$path) {
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $out = & node $path 2>&1
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    return @{ code = $code; text = ($out -join "`n") }
}

# ---------- 측정 항목 ----------
# Un과 Node가 같은 알고리즘/입력/기대값을 쓰는 쌍이다.
$Pairs = @(
    [pscustomobject]@{ label = "fib24";          un = "bench/fib.un";    node = "bench/node/fib.mjs" }
    [pscustomobject]@{ label = "nested_sum_200"; un = "bench/loop.un";   node = "bench/node/loop.mjs" }
    [pscustomobject]@{ label = "qsort500";       un = "bench/sort.un";   node = "bench/node/sort.mjs" }
    [pscustomobject]@{ label = "rle1994";        un = "bench/string.un"; node = "bench/node/string.mjs" }
    [pscustomobject]@{ label = "dict5000";       un = "bench/dict.un";   node = "bench/node/dict.mjs" }
    [pscustomobject]@{ label = "iter10000";      un = "bench/iter.un";   node = "bench/node/iter.mjs" }
    [pscustomobject]@{ label = "math10000";      un = "bench/math.un";   node = "bench/node/math.mjs" }
    [pscustomobject]@{ label = "class1000";      un = "bench/class.un";  node = "bench/node/class.mjs" }
)

if ($Rebuild) {
    Write-Host "--- building (Release) ---"
    & $rustup run $Toolchain $cargo build --release --manifest-path $Manifest
    if ($LASTEXITCODE -ne 0) {
        Write-Host "build failed"
        exit 1
    }
}

$NodeAvailable = $null -ne (Get-Command node -ErrorAction SilentlyContinue)
if ($Group -eq "all" -and -not $NodeAvailable) {
    Write-Host "node is not installed, running un only"
    $Group = "un"
}

if ($Group -notin @("all", "un", "node")) {
    Write-Host "unknown group: $Group"
    exit 2
}

$Stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$ResultsDir = Join-Path $Root "bench\results\$Stamp"
if (-not $NoSave) {
    New-Item -ItemType Directory -Path $ResultsDir -Force | Out-Null
}

Write-Host "Un benchmark runner - group: $Group"
Write-Host "executable: $Exe"

# ---------- 환경 ----------
$EnvLines = @()
foreach ($line in ((Invoke-Un "bench/env.un").text -split "`n")) {
    if ($line -match "ENV|") { $EnvLines += $line }
}
if ($Group -ne "un") {
    foreach ($line in ((Invoke-Node "bench/node/env.mjs").text -split "`n")) {
        if ($line -match "ENV|") { $EnvLines += $line }
    }
}
foreach ($line in $EnvLines) { Write-Host $line }

# ---------- 실행 ----------
$Failures = 0
$Rows = @()

foreach ($p in $Pairs) {
    $row = [pscustomobject]@{
        label = $p.label; un = $null; node = $null; mem = $null
        unValid = "skipped"; nodeValid = "skipped"; unCode = "n/a"; nodeCode = "n/a"
    }
    if ($Group -ne "node") {
        $run = Invoke-Un $p.un
        $row.unCode = $run.code
        $row.unValid = Get-Validation $run.text
        if ($run.code -ne 0) {
            Write-Host "[BAD]  $($p.un) exit=$($run.code)"
            $Failures += 1
        }
        foreach ($line in ($run.text -split "`n")) {
            $b = Parse-BenchLine $line
            if ($b) { $row.un = $b }
        }
    }
    if ($Group -ne "un") {
        $run = Invoke-Node $p.node
        $row.nodeCode = $run.code
        $row.nodeValid = Get-Validation $run.text
        if ($run.code -ne 0) {
            Write-Host "[BAD]  $($p.node) exit=$($run.code)"
            $Failures += 1
        }
        foreach ($line in ($run.text -split "`n")) {
            $b = Parse-BenchLine $line
            if ($b) { $row.node = $b }
            $m = Parse-MemLine $line
            if ($m) { $row.mem = $m }
        }
    }
    if (-not $row.un -and -not $row.node) {
        Write-Host "[BAD]  no BENCH line for $($p.label)"
        $Failures += 1
    }
    $Rows += $row
}

# ---------- 표 ----------
function RatioOf($row) {
    if ($row.un -and $row.node -and $row.node.median -gt 0) {
        return Fmt ($row.un.median / $row.node.median) "F1"
    }
    return "n/a"
}

function ItersOf($row) {
    if ($row.un) { return $row.un.iters }
    if ($row.node) { return $row.node.iters }
    return 0
}

$Csv = @()
$Csv += "label,iters,reps,warmup,un_min_ms,un_median_ms,un_avg_ms,un_max_ms,un_stddev_ms,un_ops,node_min_ms,node_median_ms,node_avg_ms,node_max_ms,node_stddev_ms,node_ops,ratio,node_heap_delta_mb,node_heap_mb,un_validation,node_validation"

Write-Host ""
Write-Host ("{0,-15} {1,6} {2,10} {3,9} {4,10} {5,10} {6,12} {7,11} {8,12} {9,9} {10,7} {11,10}" -f `
    "label", "iters", "un median", "un avg", "un stddev", "un ops", "node median", "node avg", "node stddev", "node ops", "ratio", "node heap+")

foreach ($row in $Rows) {
    $u = $row.un
    $n = $row.node
    $ratio = RatioOf $row

    $Cells = @(
        $row.label,
        (ItersOf $row),
        $(if ($u) { $u.reps } elseif ($n) { $n.reps } else { 0 }),
        $(if ($u) { $u.warmup } elseif ($n) { $n.warmup } else { 0 }),
        $(if ($u) { Fmt $u.min "F4" } else { "n/a" }),
        $(if ($u) { Fmt $u.median "F4" } else { "n/a" }),
        $(if ($u) { Fmt $u.avg "F4" } else { "n/a" }),
        $(if ($u) { Fmt $u.max "F4" } else { "n/a" }),
        $(if ($u) { Fmt $u.stddev "F4" } else { "n/a" }),
        $(if ($u) { Fmt $u.ops "F1" } else { "n/a" }),
        $(if ($n) { Fmt $n.min "F4" } else { "n/a" }),
        $(if ($n) { Fmt $n.median "F4" } else { "n/a" }),
        $(if ($n) { Fmt $n.avg "F4" } else { "n/a" }),
        $(if ($n) { Fmt $n.max "F4" } else { "n/a" }),
        $(if ($n) { Fmt $n.stddev "F4" } else { "n/a" }),
        $(if ($n) { Fmt $n.ops "F1" } else { "n/a" }),
        $ratio,
        $(if ($row.mem) { Fmt $row.mem.delta "F3" } else { "n/a" }),
        $(if ($row.mem) { Fmt $row.mem.heap "F3" } else { "n/a" }),
        $row.unValid,
        $row.nodeValid
    )
    $Csv += ($Cells -join ",")

    Write-Host ("{0,-15} {1,6} {2,10} {3,9} {4,10} {5,10} {6,12} {7,11} {8,12} {9,9} {10,7} {11,10}" -f `
        $row.label,
        (ItersOf $row),
        $(if ($u) { Fmt $u.median "F3" } else { "n/a" }),
        $(if ($u) { Fmt $u.avg "F3" } else { "n/a" }),
        $(if ($u) { Fmt $u.stddev "F3" } else { "n/a" }),
        $(if ($u) { Fmt $u.ops "F0" } else { "n/a" }),
        $(if ($n) { Fmt $n.median "F3" } else { "n/a" }),
        $(if ($n) { Fmt $n.avg "F3" } else { "n/a" }),
        $(if ($n) { Fmt $n.stddev "F3" } else { "n/a" }),
        $(if ($n) { Fmt $n.ops "F0" } else { "n/a" }),
        $ratio,
        $(if ($row.mem) { Fmt $row.mem.delta "F2" } else { "n/a" })
    )
}

# ---------- 저장 ----------
if (-not $NoSave) {
    Set-Content -Path (Join-Path $ResultsDir "results.csv") -Value $Csv -Encoding utf8

    $Md = @()
    $Md += "# 벤치마크 결과 ($Stamp)"
    $Md += ""
    $Md += "## 환경"
    $Md += ""
    foreach ($line in $EnvLines) { $Md += "- ``$line``" }
    $Md += "- OS: Windows (win32). Un은 Rust로 작성되어 빌드 산출물(unvm.exe)이 실행된다."
    $Md += "- Un 타이머: time.stopwatch().tick()는 0.1ms 단위 정수. Node: performance.now()."
    $Md += "- warmup=2, reps=7. Median=times[n/2], StdDev=모집단 표준편차, OPS=iters/(median ms/1000)."
    $Md += "- 측정 구간에는 출력이 포함되지 않는다. 각 스크립트는 측정 전에 검증한다."
    $Md += "- Un과 Node는 같은 알고리즘/입력/기대값을 쓴다(스크립트 안에서 검증)."
    $Md += "- Un에 메모리 API가 없어서 heap 열은 Node 힙 증가분만 기록한다. GC 시점 비제어."
    $Md += ""
    $Md += "| label | iters | reps | warmup | Un min | Un median | Un avg | Un max | Un stddev | Un OPS | Node min | Node median | Node avg | Node max | Node stddev | Node OPS | ratio(Un/Node) | Node heap delta MB | Un validation | Node validation |"
    $Md += "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    foreach ($row in $Rows) {
        $u = $row.un
        $n = $row.node
        $cells = @(
            $row.label,
            (ItersOf $row),
            $(if ($u) { $u.reps } elseif ($n) { $n.reps } else { 0 }),
            $(if ($u) { $u.warmup } elseif ($n) { $n.warmup } else { 0 }),
            $(if ($u) { Fmt $u.min "F4" } else { "n/a" }),
            $(if ($u) { Fmt $u.median "F4" } else { "n/a" }),
            $(if ($u) { Fmt $u.avg "F4" } else { "n/a" }),
            $(if ($u) { Fmt $u.max "F4" } else { "n/a" }),
            $(if ($u) { Fmt $u.stddev "F4" } else { "n/a" }),
            $(if ($u) { Fmt $u.ops "F1" } else { "n/a" }),
            $(if ($n) { Fmt $n.min "F4" } else { "n/a" }),
            $(if ($n) { Fmt $n.median "F4" } else { "n/a" }),
            $(if ($n) { Fmt $n.avg "F4" } else { "n/a" }),
            $(if ($n) { Fmt $n.max "F4" } else { "n/a" }),
            $(if ($n) { Fmt $n.stddev "F4" } else { "n/a" }),
            $(if ($n) { Fmt $n.ops "F1" } else { "n/a" }),
            (RatioOf $row),
            $(if ($row.mem) { Fmt $row.mem.delta "F3" } else { "n/a" }),
            $row.unValid,
            $row.nodeValid
        )
        $Md += ("| " + ($cells -join " | ") + " |")
    }
    Set-Content -Path (Join-Path $ResultsDir "results.md") -Value $Md -Encoding utf8

    Write-Host ""
    Write-Host "results saved to $ResultsDir"
}

if ($Failures -gt 0) { exit 1 }
exit 0
