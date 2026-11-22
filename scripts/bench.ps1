<#
.SYNOPSIS
    Un 벤치마크 러너. C:\dev\un 에서 실행한다.

.DESCRIPTION
    bench/*.un(Un VM / Un JIT / Compile Mode / standalone exe), bench/python/*.py(CPython), bench/node/*.mjs(Node.js)를
    같은 알고리즘/입력/검증으로 실행하고, 레이블별로 짝지어 비교한다.

    측정 규칙:
      - warmup을 먼저 실행하고 측정에는 포함하지 않는다.
      - 각 실행은 reps회 개별 측정하고 중앙값을 기본 비교 지표로 쓴다.
      - Min/Median/Average/Max/StdDev(모집단)와 Median 기준 OPS를 출력한다.
      - 각 스크립트는 실행 전 스스로 검증한다(Un: assert.summary(), 성공 시 "N passed",
        Node/Python: "=== Verify: ok ===").

    Un은 UV_JIT=0(VM)과 기본(JIT)으로 각각 측정한다. Python이 없으면 python 열은
    건너뛰고 경고만 출력한다(단, -Group python 지정 시엔 실패로 처리).

.PARAMETER Only
    쉼표로 구분한 벤치마크 label만 실행한다. 예: -Only qsort500,mergesort1000
    group은 언어 레인을, Only는 벤치마크를 고른다.

.PARAMETER Group
    all(기본) | un | unvm | unjit | unnative | unbuild | native | node | python
    all      = Un VM + Un JIT + Compile Mode(un native) + standalone exe(un build) + Python + Node
    un       = Un JIT만 (하위 호환)
    unvm     = Un VM만 (UV_JIT=0)
    unjit    = Un JIT만 (기본 런타임)
    unnative = Compile Mode (`un native`)만 — 별도 컬럼에 기록
    unbuild  = standalone native exe (`un build`)만 — 빌드 가능한 벤치만 해당
    native   = unnative + unbuild (compile mode 전체)
    node     = Node만
    python   = Python만

.PARAMETER Rebuild
    cargo build --release를 먼저 실행한다.

.PARAMETER NoSave
    bench/results에 저장하지 않는다.

.PARAMETER Report
    실행 결과를 바탕으로 프로젝트 루트의 BENCHMARK.md를 다시 생성한다.

.PARAMETER Compare
    지정한 results.csv와 비교하여 성능 회귀/개선을 출력한다.
    생략하면 results/ 아래의 가장 최근 결과(이번 실행 이전)와 비교한다.

.EXAMPLE
    powershell -File scripts/bench.ps1
    powershell -File scripts/bench.ps1 -Group unjit -NoSave
    powershell -File scripts/bench.ps1 -Group all -Report
#>
param(
    [string]$Group = "all",
    [string]$Only = "",
    [switch]$Rebuild,
    [switch]$NoSave,
    [switch]$Report,
    [string]$Compare = ""
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$rustup = Join-Path $env:USERPROFILE ".cargo\bin\rustup.exe"
$cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
$Exe = Join-Path $Root "rust\target\release\un.exe"
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

function Parse-BareBench([string]$text, [int]$iters, [int]$reps, [int]$warmup) {
    # Compile-mode fixtures are self-timed and print bare millisecond values
    # (one per repetition); synthesize a BENCH-shaped record for the tables.
    $samples = @()
    foreach ($line in ($text -split "`n")) {
        $v = 0.0
        $t = $line.Trim()
        if ($t -and [double]::TryParse($t, [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$v)) { $samples += $v }
    }
    if ($samples.Count -eq 0) { return $null }
    $sorted = @($samples | Sort-Object)
    $sum = 0.0; foreach ($v in $sorted) { $sum += $v }
    $avg = $sum / $sorted.Count
    $dev = 0.0; foreach ($v in $sorted) { $dev += ($v - $avg) * ($v - $avg) }
    $median = $sorted[[int]($sorted.Count / 2)]
    return @{
        iters = $iters; reps = $samples.Count; warmup = $warmup
        min = $sorted[0]; median = $median; avg = $avg
        max = $sorted[$sorted.Count - 1]; stddev = [math]::Sqrt($dev / $sorted.Count)
        ops = if ($median -gt 0) { 1000.0 / $median } else { 0.0 }
    }
}

function Parse-MemLine([string]$line) {
    $i = $line.IndexOf("MEM|")
    if ($i -lt 0) { return $null }
    $parts = $line.Substring($i).Split("|")
    if ($parts.Count -lt 4) { return $null }
    return @{ label = $parts[1]; delta = [double]$parts[2]; heap = [double]$parts[3] }
}

# ---------- 통계 유틸 ----------
function Get-Stats([double[]]$values) {
    if ($values.Count -lt 2) { return $null }
    $sorted = @($values | Sort-Object)
    $n = $sorted.Count
    $mean = ($values | Measure-Object -Average).Average
    $sd = [math]::Sqrt((($values | ForEach-Object { ($_ - $mean) * ($_ - $mean) } | Measure-Object -Sum).Sum) / ($n - 1))
    $cv = if ($mean -gt 0) { $sd / $mean } else { 0.0 }
    $p95 = $sorted[[math]::Min($n - 1, [int][math]::Ceiling(0.95 * $n) - 1)]
    # Grubbs 이상치 검정(양측 95%, n=7 임계 ≈ 2.02)
    $grubbsCritical = @{ 3=1.15; 4=1.46; 5=1.67; 6=1.82; 7=1.94; 8=2.03; 9=2.11; 10=2.18 }
    $crit = if ($grubbsCritical.ContainsKey($n)) { $grubbsCritical[$n] } else { 2.2 }
    $outliers = @()
    if ($sd -gt 0) {
        for ($i = 0; $i -lt $n; $i++) { if ([math]::Abs($values[$i] - $mean) / $sd -gt $crit) { $outliers += $i } }
    }
    # 95% 신뢰구간 (t distribution 근사, n=7 → t≈2.447)
    $tcrit = @{ 3=4.303; 4=3.182; 5=2.776; 6=2.571; 7=2.447; 8=2.365; 9=2.306; 10=2.262 }
    $t = if ($tcrit.ContainsKey($n)) { $tcrit[$n] } else { 2.2 }
    $margin = $t * $sd / [math]::Sqrt($n)
    return @{ mean=$mean; sd=$sd; cv=$cv; p95=$p95; outliers=$outliers.Count; ci95Lo=$mean-$margin; ci95Hi=$mean+$margin }
}

function Get-Samples([string]$text) {
    $samples = @()
    foreach ($line in ($text -split "`n")) {
        if ($line -match '^SAMPLE\|') {
            $v = 0.0
            if ([double]::TryParse($line.Split('|')[1], [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$v)) { $samples += $v }
        }
    }
    return $samples
}
function Canonical-Result([string]$value) {
    # 결과 정규화:
    #  - 정수/실수: 수치 비교(부동소수점 마지막 자리 허용오차), 정수값 float은 int로 통합
    #  - 리스트/맵: 공백 제거 후 문자열 비교
    $v = $value.Trim()
    $d = 0.0
    if ([double]::TryParse($v, [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$d)) {
        if ([math]::Abs($d - [math]::Round($d)) -lt 1e-9 -and [math]::Abs($d) -lt 9e15) {
            return "n:" + [int64][math]::Round($d)
        }
        return "n:" + [math]::Round($d, 9).ToString("F9", [Globalization.CultureInfo]::InvariantCulture)
    }
    if ($v.StartsWith('[') -or $v.StartsWith('{')) { return "c:" + (($v -replace '\s', '')) }
    return "s:" + $v
}
function Get-Results([string]$text) {
    # BENCH 결과 외에 각 스크립트가 출력하는 RESULT|label|value 줄을 모은다.
    $map = @{}
    foreach ($line in ($text -split "`n")) {
        if ($line -match '^RESULT\|([^|]+)\|(.*)$') {
            $canon = Canonical-Result $Matches[2]
            # 수치 결과만 cross-lane 비교 (리스트/맵은 언어별 출력 형식이 다름)
            if ($canon.StartsWith("n:")) { $map[$Matches[1]] = $canon }
        }
    }
    return $map
}
function Get-Validation([string]$text) {
    foreach ($line in $text -split "`n") {
        if ($line -match "=== Result: (\d+) passed, (\d+) failed ===") {
            return "{0} passed / {1} failed" -f $Matches[1], $Matches[2]
        }
        if ($line -match "=== Verify: ok ===") {
            return "verify ok"
        }
    }
    return "none"
}

function Invoke-Un([string]$path, [bool]$jitOff = $false) {
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    # UV_JIT=0이면 VM만, 기본이면 현재 런타임(=JIT)으로 실행된다.
    if ($jitOff) { $env:UV_JIT = "0" } else { $env:UV_JIT = $null }
    $out = & $Exe run $path 2>&1
    $code = $LASTEXITCODE
    Remove-Item Env:UV_JIT -ErrorAction SilentlyContinue
    $ErrorActionPreference = $prev
    return @{ code = $code; text = ($out -join "`n") }
}

function Invoke-UnNative([string]$path) {
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $out = & $Exe native $path 2>&1
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    return @{ code = $code; text = ($out -join "`n") }
}

function Invoke-BuildExe([string]$source, [string]$exePath) {
    # Compile a plain standalone exe, run it 7 times and parse the bare float
    # each run prints (ms per workload unit). The dedicated .un fixture avoids
    # bench.harness/assert imports because those are outside the plain-exe core.
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & $Exe build $source -o $exePath *> $null
    if ($LASTEXITCODE -ne 0) {
        $ErrorActionPreference = $prev
        return @{ code = $LASTEXITCODE; text = "" }
    }
    & $exePath *> $null   # warmup
    $text = ""
    for ($r = 0; $r -lt 7; $r++) {
        $out = & $exePath 2>&1
        if ($LASTEXITCODE -ne 0) {
            $ErrorActionPreference = $prev
            return @{ code = $LASTEXITCODE; text = ($out -join "`n") }
        }
        $text += ($out -join "`n") + "`n"
    }
    $ErrorActionPreference = $prev
    return @{ code = 0; text = $text }
}

function Invoke-Node([string]$path) {
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $out = & node $path 2>&1
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    return @{ code = $code; text = ($out -join "`n") }
}

function Invoke-Python([string]$path, [string]$pythonExe) {
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    if ($null -ne $pythonExe) { $out = & $pythonExe $path 2>&1 } else { $out = & python $path 2>&1 }
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    return @{ code = $code; text = ($out -join "`n") }
}

# ---------- 측정 항목 ----------
# Un/Node/Python이 같은 알고리즘/입력/기대값을 쓰는 트리플이다.
$Pairs = @(
    [pscustomobject]@{ label = "int_ops";        un = "bench/int_ops.un";   native = "bench/native_int_ops.un"; exe = "bench/native_int_ops.un"; node = "bench/node/int_ops.mjs";   python = "bench/python/int_ops.py" }
    [pscustomobject]@{ label = "fib24";          un = "bench/fib.un";       native = "bench/native_fib_exe.un"; exe = "bench/native_fib_exe.un"; node = "bench/node/fib.mjs";       python = "bench/python/fib.py" }
    [pscustomobject]@{ label = "native_fib24";   un = "bench/native_fib.un"; native = "bench/native_fib_exe.un"; exe = "bench/native_fib_exe.un"; node = "bench/node/fib.mjs";      python = "bench/python/fib.py" }
    [pscustomobject]@{ label = "nested_sum_200"; un = "bench/loop.un";      native = "bench/native_loop.un"; exe = "bench/native_loop.un"; node = "bench/node/loop.mjs";     python = "bench/python/loop.py" }
    [pscustomobject]@{ label = "qsort500";       un = "bench/sort.un";      native = "bench/native_sort.un"; exe = "bench/native_sort.un"; node = "bench/node/sort.mjs";     python = "bench/python/sort.py" }
    [pscustomobject]@{ label = "mergesort1000";  un = "bench/mergesort.un"; native = "bench/native_mergesort.un"; exe = "bench/native_mergesort.un"; node = "bench/node/mergesort.mjs"; python = "bench/python/mergesort.py" }
    [pscustomobject]@{ label = "rle1994";        un = "bench/string.un";    node = "bench/node/string.mjs";   python = "bench/python/string.py" }
    [pscustomobject]@{ label = "dict5000";       un = "bench/dict.un";      native = "bench/native_dict.un"; exe = "bench/native_dict.un"; node = "bench/node/dict.mjs";     python = "bench/python/dict.py" }
    [pscustomobject]@{ label = "iter10000";      un = "bench/iter.un";      native = "bench/native_iter.un"; exe = "bench/native_iter.un"; node = "bench/node/iter.mjs";     python = "bench/python/iter.py" }
    [pscustomobject]@{ label = "math10000";      un = "bench/math.un";      native = "bench/native_math.un"; exe = "bench/native_math.un"; node = "bench/node/math.mjs";     python = "bench/python/math.py" }
    [pscustomobject]@{ label = "class1000";      un = "bench/class.un";     native = "bench/native_class.un"; exe = "bench/native_class.un"; node = "bench/node/class.mjs";    python = "bench/python/class.py" }
    [pscustomobject]@{ label = "sieve30000";     un = "bench/sieve.un";     native = "bench/native_sieve.un"; exe = "bench/native_sieve.un"; node = "bench/node/sieve.mjs";    python = "bench/python/sieve.py" }
    [pscustomobject]@{ label = "matrix48";       un = "bench/matrix.un";    node = "bench/node/matrix.mjs";   python = "bench/python/matrix.py" }
    [pscustomobject]@{ label = "sin20000";       un = "bench/floatmath.un"; native = "bench/native_floatmath.un"; exe = "bench/native_floatmath.un"; node = "bench/node/floatmath.mjs"; python = "bench/python/floatmath.py" }
    [pscustomobject]@{ label = "call200000";     un = "bench/funccall.un";  native = "bench/native_funccall.un"; exe = "bench/native_funccall.un"; node = "bench/node/funccall.mjs";  python = "bench/python/funccall.py" }
    [pscustomobject]@{ label = "regex6000";      un = "bench/regex.un";     node = "bench/node/regex.mjs";    python = "bench/python/regex.py" }
    [pscustomobject]@{ label = "str_unicode";    un = "bench/str_unicode.un";    node = "bench/node/str_unicode.mjs";    python = "bench/python/str_unicode.py" }
    [pscustomobject]@{ label = "except10000";   un = "bench/exceptions.un";     node = "bench/node/exceptions.mjs";    python = "bench/python/exceptions.py" }
    [pscustomobject]@{ label = "match20000";    un = "bench/match.un";          node = "bench/node/match.mjs";         python = "bench/python/match.py" }
    [pscustomobject]@{ label = "closure20000";  un = "bench/closure.un";        node = "bench/node/closure.mjs";       python = "bench/python/closure.py" }
    [pscustomobject]@{ label = "inherit1000";   un = "bench/inheritance.un";   node = "bench/node/inheritance.mjs"; python = "bench/python/inheritance.py" }
    [pscustomobject]@{ label = "set5000";       un = "bench/set.un";            node = "bench/node/set.mjs";           python = "bench/python/set.py" }
    [pscustomobject]@{ label = "enum10000";     un = "bench/enum.un";           node = "bench/node/enum.mjs";          python = "bench/python/enum_bench.py" }
    [pscustomobject]@{ label = "modimport2000"; un = "bench/modimport.un";      node = "bench/node/modimport.mjs";     python = "bench/python/modimport.py" }
    [pscustomobject]@{ label = "nanbox20000";   un = "bench/nanbox.un";         node = "bench/node/nanbox.mjs";        python = "bench/python/nanbox.py" }
)

# ---------- 옵션: 벤치마크 필터 ----------
if ($Only -ne "") {
    $wanted = $Only -split ',' | ForEach-Object { $_.Trim().ToLower() } | Where-Object { $_ }
    $known = $Pairs | ForEach-Object { $_.label }
    $missing = $wanted | Where-Object { $_ -notin $known }
    if ($missing) {
        Write-Host "unknown benchmark label(s): $($missing -join ', ')"
        Write-Host "valid labels: $($known -join ', ')"
        exit 2
    }
    $Pairs = @($Pairs | Where-Object { $wanted -contains $_.label })
}

# ---------- 그룹 ----------
$eachMode = @()
foreach ($g in ($Group -split ',' | ForEach-Object { $_.Trim().ToLower() })) {
    switch ($g) {
        "all"   { $eachMode += @("unvm", "unjit", "python", "node", "unnative", "unbuild") }
        "un"    { $eachMode += "unjit" }   # 하위 호환: un == unjit
        "unvm"  { $eachMode += "unvm" }
        "unjit" { $eachMode += "unjit" }
        # compile-mode(`un native`): 정적분석(컴파일) 기반 네이티브 경로만 별도로 측정한다.
        # 결과는 unjit 컬럼에 담지만 -Group 이름은 native/unnative로 선택한다.
        "unnative" { $eachMode += "unnative" }
        "unbuild"  { $eachMode += "unbuild" }
        "native"   { $eachMode += @("unnative", "unbuild") }
        "node"  { $eachMode += "node" }
        "python"{ $eachMode += "python" }
        default {
            Write-Host "unknown group: $g (valid: all | un | unvm | unjit | unnative | unbuild | native | node | python)"
            exit 2
        }
    }
}

if ($Rebuild) {
    Write-Host "--- building (Release) ---"
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

# ---------- 인터프리터 탐지 ----------
$NodeAvailable = $null -ne (Get-Command node -ErrorAction SilentlyContinue)
# python on PATH는 WindowsApps stub(9009)일 수 있으므로 exit 코드로 확인한다.
$PythonExe = $null
$pythonVersionLine = ""
try {
    $pv = & python --version 2>&1
    if ($LASTEXITCODE -eq 0 -and "$pv" -match "Python") { $PythonExe = "python"; $pythonVersionLine = "$pv" }
} catch { }
if ($null -eq $PythonExe) {
    $candidate = Join-Path $env:LOCALAPPDATA "Programs\Python\Python313\python.exe"
    if (Test-Path $candidate) {
        $pv = & $candidate --version 2>&1
        if ($LASTEXITCODE -eq 0) { $PythonExe = $candidate; $pythonVersionLine = "$pv" }
    }
}
$PythonAvailable = $null -ne $PythonExe
if ($Group -eq "python" -and -not $PythonAvailable) {
    Write-Host "python is not available (or the PATH entry is the broken WindowsApps stub)" -ForegroundColor Red
    exit 1
}
if ($eachMode -contains "node" -and -not $NodeAvailable) {
    Write-Host "warning: node not found, skipping node rows" -ForegroundColor Yellow
    $eachMode = $eachMode | Where-Object { $_ -ne "node" }
}
if ($eachMode -contains "python" -and -not $PythonAvailable) {
    if ($Group -eq "all") {
        Write-Host "warning: python not available, skipping python rows" -ForegroundColor Yellow
        $eachMode = $eachMode | Where-Object { $_ -ne "python" }
    } else {
        Write-Host "python is not available (or the PATH entry is the broken WindowsApps stub)" -ForegroundColor Red
        exit 1
    }
}

$Stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$ResultsDir = Join-Path $Root "bench\results\$Stamp"
if (-not $NoSave) { New-Item -ItemType Directory -Path $ResultsDir -Force | Out-Null }

Write-Host "Un benchmark runner - modes: $($eachMode -join ',')"
Write-Host "executable: $Exe"
if ($PythonAvailable) { Write-Host "python: $pythonVersionLine" }
if ($NodeAvailable)   { Write-Host "node:   $(node --version)" }

# ---------- 환경 ----------
$EnvLines = @()
foreach ($line in ((Invoke-Un "bench/env.un" $false).text -split "`n")) {
    if ($line -match "ENV\|") { $EnvLines += $line }
}
if ($eachMode -contains "node") {
    foreach ($line in ((Invoke-Node "bench/node/env.mjs").text -split "`n")) {
        if ($line -match "ENV\|") { $EnvLines += $line }
    }
}
if ($eachMode -contains "python") {
    foreach ($line in ((Invoke-Python "bench/python/env.py" $PythonExe).text -split "`n")) {
        if ($line -match "ENV\|") { $EnvLines += $line }
    }
}
try { $commit = (& git rev-parse --short HEAD 2>&1 | Select-Object -First 1) } catch { $commit = "unknown" }
$EnvLines += "ENV|un_commit|$commit"
foreach ($line in $EnvLines) { Write-Host $line }

# ---------- 실행 ----------
$Failures = 0
$Rows = @()
$compileOnly = -not @($eachMode | Where-Object { $_ -notin @("unnative", "unbuild") }).Count

foreach ($p in $Pairs) {
    $row = [pscustomobject]@{
        label = $p.label; unvm = $null; unjit = $null; compile = $null; build = $null; python = $null; node = $null; memNode = $null; memPython = $null; unvmResult = $null; unjitResult = $null; compileResult = $null; buildResult = $null; pyResult = $null; nodeResult = $null; unvmSamples = @(); unjitSamples = @(); pySamples = @(); nodeSamples = @(); unvmStats = $null; unjitStats = $null; pyStats = $null; nodeStats = $null
        unvmCode = "n/a"; unjitCode = "n/a"; compileCode = "n/a"; buildCode = "n/a"; pyCode = "n/a"; nodeCode = "n/a"
        unvmValid = "skipped"; unjitValid = "skipped"; compileValid = "skipped"; buildValid = "skipped"; pyValid = "skipped"; nodeValid = "skipped"
    }
    if ($eachMode -contains "unvm") {
        $r = Invoke-Un $p.un $true
        $row.unvmCode = $r.code; $row.unvmValid = Get-Validation $r.text
        if ($r.code -ne 0) { Write-Host "[BAD]  VM  $($p.un) exit=$($r.code)" -ForegroundColor Red; $Failures += 1 }
        $row.unvmResult = Get-Results $r.text
        $row.unvmSamples = Get-Samples $r.text
        foreach ($line in ($r.text -split "`n")) { $b = Parse-BenchLine $line; if ($b) { $row.unvm = $b } }
    }
    if ($eachMode -contains "unnative") {
        # Compile Mode (un native): separate column, never overwrites unjit.
        $r = Invoke-UnNative $(if ($p.native) { $p.native } else { $p.un })
        $row.compileCode = $r.code; $row.compileValid = Get-Validation $r.text
        if ($r.code -ne 0) {
            if ($r.text -match "compile error:") {
                # Dynamic/untyped workloads are intentionally outside compile mode.
                $row.compileValid = "compile-rejected (skipped)"
            } else {
                Write-Host "[BAD]  native $($p.un) exit=$($r.code)" -ForegroundColor Red; $Failures += 1
            }
        }
        $row.compileResult = Get-Results $r.text
        foreach ($line in ($r.text -split "`n")) { $b = Parse-BenchLine $line; if ($b) { $row.compile = $b } }
        if (-not $row.compile -and $r.code -eq 0) {
            $row.compile = Parse-BareBench $r.text 1 7 2
            if ($row.compile) { $row.compileValid = "self-timed (no asserts)" }
        }
    }
    if ($eachMode -contains "unbuild" -and $p.exe) {
        $exePath = Join-Path $env:TEMP ("un_bench_" + $p.label + "_" + [guid]::NewGuid().ToString("N") + ".exe")
        $r = Invoke-BuildExe $p.exe $exePath
        $row.buildCode = $r.code; $row.buildValid = if ($r.code -eq 0) { "n/a (exe)" } else { "failed" }
        if ($r.code -ne 0) { Write-Host "[BAD]  build $($p.exe) exit=$($r.code)" -ForegroundColor Red; $Failures += 1 }
        if ($r.code -eq 0) {
            $row.build = Parse-BareBench $r.text 1 7 1
        }
        if (Test-Path $exePath) { Remove-Item $exePath -Force -ErrorAction SilentlyContinue }
    }
    if ($eachMode -contains "unjit") {
        $r = Invoke-Un $p.un $false
        $row.unjitCode = $r.code; $row.unjitValid = Get-Validation $r.text
        if ($r.code -ne 0) { Write-Host "[BAD]  JIT $($p.un) exit=$($r.code)" -ForegroundColor Red; $Failures += 1 }
        $row.unjitResult = Get-Results $r.text
        $row.unjitSamples = Get-Samples $r.text
        foreach ($line in ($r.text -split "`n")) { $b = Parse-BenchLine $line; if ($b) { $row.unjit = $b } }
    }
    if ($eachMode -contains "python") {
        $r = Invoke-Python $p.python $PythonExe
        $row.pyCode = $r.code; $row.pyValid = Get-Validation $r.text
        if ($r.code -ne 0) { Write-Host "[BAD]  PY  $($p.python) exit=$($r.code)" -ForegroundColor Red; $Failures += 1 }
        $row.pyResult = Get-Results $r.text
        $row.pySamples = Get-Samples $r.text
        foreach ($line in ($r.text -split "`n")) {
            $b = Parse-BenchLine $line; if ($b) { $row.python = $b }
            $m = Parse-MemLine $line; if ($m) { $row.memPython = $m }
        }
    }
    if ($eachMode -contains "node") {
        $r = Invoke-Node $p.node
        $row.nodeCode = $r.code; $row.nodeValid = Get-Validation $r.text
        if ($r.code -ne 0) { Write-Host "[BAD]  NODE $($p.node) exit=$($r.code)" -ForegroundColor Red; $Failures += 1 }
        $row.nodeResult = Get-Results $r.text
        $row.nodeSamples = Get-Samples $r.text
        foreach ($line in ($r.text -split "`n")) {
            $b = Parse-BenchLine $line; if ($b) { $row.node = $b }
            $m = Parse-MemLine $line; if ($m) { $row.memNode = $m }
        }
    }
    # ---------- 통계 계산 & 노이즈 경고 ----------
    foreach ($pair in @(@("unvm","unvmSamples","unvmStats"), @("unjit","unjitSamples","unjitStats"), @("python","pySamples","pyStats"), @("node","nodeSamples","nodeStats"))) {
        $samples = $row.($pair[1])
        if ($samples.Count -ge 2) {
            $st = Get-Stats ([double[]]$samples)
            $row.($pair[2]) = $st
            if ($st.cv -gt 0.15 -or $st.outliers -gt 0) {
                Write-Host ("[NOISE] {0}/{1} cv={2:F3} outliers={3} ci95=[{4:F3},{5:F3}]" -f $p.label, $pair[0], $st.cv, $st.outliers, $st.ci95Lo, $st.ci95Hi) -ForegroundColor Yellow
            }
        }
    }
    # ---------- cross-lane 결과 대조 ----------
    # 모든 레인이 같은 RESULT|label|value 를 출력해야 한다. 한 레인이라도 다르면 실패.
    $laneResults = @{}
    foreach ($entry in @(@("vm",$row.unvmResult), @("jit",$row.unjitResult), @("compile",$row.compileResult), @("exe",$row.buildResult), @("python",$row.pyResult), @("node",$row.nodeResult))) {
        if ($null -eq $entry[1]) { continue }
        foreach ($key in $entry[1].Keys) {
            if (-not $laneResults.ContainsKey($key)) { $laneResults[$key] = @{} }
            $laneResults[$key][$entry[0]] = $entry[1][$key]
        }
    }
    foreach ($key in $laneResults.Keys) {
        $values = @($laneResults[$key].Values | Sort-Object -Unique)
        if ($values.Count -gt 1) {
            $detail = ($laneResults[$key].GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join " "
            Write-Host "[XLANE]  $($p.label)/$key  $detail" -ForegroundColor Red
            $Failures += 1
        }
    }
    if (-not $compileOnly -and -not $row.unvm -and -not $row.unjit -and -not $row.compile -and -not $row.build -and -not $row.python -and -not $row.node) {
        Write-Host "[BAD]  no BENCH line for $($p.label)" -ForegroundColor Red
        $Failures += 1
    }
    $Rows += $row
}

# ---------- 지표 계산 ----------
function ItersOf($row) {
    foreach ($k in @("unjit", "compile", "build", "unvm", "python", "node")) {
        if ($row.$k) { return $row.$k.iters }
    }
    return 0
}
function BitOf($row) {
    foreach ($k in @("unjit", "compile", "build", "unvm", "python", "node")) {
        if ($row.$k) { return @{ reps = $row.$k.reps; warmup = $row.$k.warmup } }
    }
    return @{ reps = 0; warmup = 0 }
}
function RatioOf($candidate, $baseline) {
    # baseline_ms / candidate_ms — 1.00x=동일, 1.50x=후보가 기준보다 1.5배 빠름, 0.50x=2배 느림.
    if ($candidate -and $baseline -and $candidate.median -gt 0) {
        return $baseline.median / $candidate.median
    }
    return $null
}

function Geomean([double[]]$values) {
    if ($values.Count -eq 0) { return $null }
    $p = 1.0
    foreach ($v in $values) { $p *= $v }
    return [math]::Pow($p, 1.0 / $values.Count)
}

# ---------- 표 ----------
Write-Host ""
Write-Host ("{0,-15} {1,6} {2,10} {3,10} {4,11} {5,9} {6,10} {7,10} {8,8} {9,8} {10,8} {11,8}" -f `
    "label", "iters", "un vm(ms)", "un jit(ms)", "native(ms)", "exe(ms)", "python(ms)", "node(ms)", "vm/py", "jit/py", "jit/node", "py/node")

function FmtOrNA($v, $f) {
    if ($null -eq $v) { return "n/a" }
    return Fmt $v $f
}
function MedianOf($langset) {
    if ($null -eq $langset) { return $null }
    return $langset.median
}
foreach ($row in $Rows) {
    $bit = BitOf $row
    $rVMPY  = RatioOf $row.unvm $row.python
    $rJITPY = RatioOf $row.unjit $row.python
    $rJITN  = RatioOf $row.unjit $row.node
    $rPYN   = RatioOf $row.python $row.node
    $vmMedian = MedianOf $row.unvm
    $jitMedian = MedianOf $row.unjit
    $compileMedian = MedianOf $row.compile
    $buildMedian = MedianOf $row.build
    $pyMedian = MedianOf $row.python
    $nodeMedian = MedianOf $row.node
    Write-Host ("{0,-15} {1,6} {2,10} {3,10} {4,11} {5,9} {6,10} {7,10} {8,8} {9,8} {10,8} {11,8}" -f `
        $row.label, (ItersOf $row),
        (FmtOrNA $vmMedian "F3"), (FmtOrNA $jitMedian "F3"),
        (FmtOrNA $compileMedian "F3"), (FmtOrNA $buildMedian "F3"),
        (FmtOrNA $pyMedian "F3"), (FmtOrNA $nodeMedian "F3"),
        (FmtOrNA $rVMPY "F3"), (FmtOrNA $rJITPY "F3"), (FmtOrNA $rJITN "F3"), (FmtOrNA $rPYN "F3"))
}

# ---------- 비교(회귀) ----------
$PreviousCsv = $Compare
if ($PreviousCsv -eq "" -and -not $NoSave -and (Test-Path (Join-Path $Root "bench\results"))) {
    $candidates = Get-ChildItem (Join-Path $Root "bench\results") -Directory |
        Where-Object { $_.Name -ne $Stamp } |
        Sort-Object Name -Descending
    foreach ($cand in $candidates) {
        $test = Join-Path $cand.FullName "results.csv"
        if (Test-Path $test) { $PreviousCsv = $test; break }
    }
}
$Regressions = @()
$Comparisons = @()
if ($PreviousCsv -and (Test-Path $PreviousCsv)) {
    $prev = @{}
    $prevLines = Get-Content $PreviousCsv
    $headers = $prevLines[0] -split ","
    $colIndex = @{}
    for ($i = 0; $i -lt $headers.Count; $i++) { $colIndex[$headers[$i].Trim()] = $i }
    $colFor = {
        param($name)
        if ($colIndex.ContainsKey($name)) { return $colIndex[$name] }
        return $null
    }
    $iUnjit = & $colFor "unjit_median_ms"
    if ($null -eq $iUnjit) { $iUnjit = & $colFor "un_median_ms" }   # 이전 포맷 폴백
    $iCompile = & $colFor "compile_median_ms"
    $iBuild = & $colFor "build_median_ms"
    $iPython = & $colFor "python_median_ms"
    $iNode = & $colFor "node_median_ms"
    foreach ($line in ($prevLines | Select-Object -Skip 1)) {
        $parts = $line -split ","
        if ($parts.Count -lt 10) { continue }
        $get = { param($idx) if ($null -eq $idx) { $null } else { $v = $parts[$idx]; if ($v -match '^-?\d+(\.\d+)?$') { [double]::Parse($v, [System.Globalization.CultureInfo]::InvariantCulture) } else { $null } } }
        $prev[$parts[0]] = @{ unjit = (& $get $iUnjit); compile = (& $get $iCompile); build = (& $get $iBuild); python = (& $get $iPython); node = (& $get $iNode) }
    }
    foreach ($row in $Rows) {
        if (-not $prev.ContainsKey($row.label)) { continue }
        $p = $prev[$row.label]
        $cur = @{
            unjit = if ($row.unjit) { $row.unjit.median } else { $null }
            compile = if ($row.compile) { $row.compile.median } else { $null }
            build = if ($row.build) { $row.build.median } else { $null }
            python = if ($row.python) { $row.python.median } else { $null }
            node = if ($row.node) { $row.node.median } else { $null }
        }
        foreach ($lang in @("unjit", "compile", "build", "python", "node")) {
            if ($null -ne $cur[$lang] -and $null -ne $p[$lang] -and $p[$lang] -gt 0) {
                $change = ($cur[$lang] - $p[$lang]) / $p[$lang] * 100.0
                $Comparisons += [pscustomobject]@{ label = $row.label; lang = $lang; pct = $change }
                if ($change -gt 10) { $Regressions += [pscustomobject]@{ label = $row.label; lang = $lang; pct = $change } }
            }
        }
    }
}
if ($Comparisons.Count -gt 0) {
    Write-Host ""
    Write-Host "Regression vs $PreviousCsv (positive = slower):"
    foreach ($c in $Comparisons | Sort-Object pct -Descending) {
        $mark = if ($c.pct -gt 10) { "REGRESSION" } elseif ($c.pct -lt -10) { "improved  " } else { "          " }
        Write-Host ("{0,-15} {1,-8} {2:F1}%  {3}" -f $c.label, $c.lang, $c.pct, $mark)
    }
    if ($Regressions.Count -gt 0) {
        Write-Host "$($Regressions.Count) regression(s) > 10% detected (advisory, not failing)" -ForegroundColor Yellow
    }
}

# ---------- 통계 요약 출력 ----------
Write-Host ""
Write-Host "Statistical summary (median, CV, 95% CI, outliers):"
foreach ($row in $Rows) {
    foreach ($pair in @(@("unvm","unvmStats"), @("unjit","unjitStats"), @("python","pyStats"), @("node","nodeStats"))) {
        $st = $row.($pair[1])
        if ($null -eq $st) { continue }
        $med = $row.($pair[0]).median
        Write-Host ("  {0,-15} {1,-7} med={2,10:F3} cv={3,7:F3} p95={4,10:F3} ci95=[{5:F3},{6:F3}] out={7}" -f `
            $row.label, $pair[0], $med, $st.cv, $st.p95, $st.ci95Lo, $st.ci95Hi, $st.outliers)
    }
}
# ---------- 저장 ----------
$Csv = @()
$Header = "label,iters,reps,warmup,unvm_min_ms,unvm_median_ms,unvm_avg_ms,unvm_max_ms,unvm_stddev_ms,unvm_ops,unjit_min_ms,unjit_median_ms,unjit_avg_ms,unjit_max_ms,unjit_stddev_ms,unjit_ops,compile_min_ms,compile_median_ms,compile_avg_ms,compile_max_ms,compile_stddev_ms,compile_ops,build_min_ms,build_median_ms,build_avg_ms,build_max_ms,build_stddev_ms,build_ops,python_min_ms,python_median_ms,python_avg_ms,python_max_ms,python_stddev_ms,python_ops,node_min_ms,node_median_ms,node_avg_ms,node_max_ms,node_stddev_ms,node_ops,python_heap_delta_mb,python_heap_mb,node_heap_delta_mb,unvm_validation,unjit_validation,compile_validation,build_validation,python_validation,node_validation"
$Csv += $Header

foreach ($row in $Rows) {
    $b = BitOf $row
    $cells = @($row.label, (ItersOf $row), $b.reps, $b.warmup)
    foreach ($k in @("unvm", "unjit", "compile", "build", "python", "node")) {
        $v = $row.$k
        if ($null -ne $v) {
            $sMin = Fmt $v.min "F6"; $sMed = Fmt $v.median "F6"; $sAvg = Fmt $v.avg "F6"
            $sMax = Fmt $v.max "F6"; $sStd = Fmt $v.stddev "F6"; $sOps = Fmt $v.ops "F1"
            $cells += @($sMin, $sMed, $sAvg, $sMax, $sStd, $sOps)
        } else {
            $cells += @("n/a", "n/a", "n/a", "n/a", "n/a", "n/a")
        }
    }
    $cells += $(if ($row.memPython) { Fmt $row.memPython.delta "F3" } else { "n/a" })
    $cells += $(if ($row.memPython) { Fmt $row.memPython.heap "F3" } else { "n/a" })
    $cells += $(if ($row.memNode) { Fmt $row.memNode.delta "F3" } else { "n/a" })
    $cells += @($row.unvmValid, $row.unjitValid, $row.compileValid, $row.buildValid, $row.pyValid, $row.nodeValid)
    $Csv += ($cells -join ",")
}

$Json = @{
    stamp    = $Stamp
    commit   = $commit
    args     = $PSBoundParameters
    modes    = $eachMode
    env      = $EnvLines
    rows     = $Rows | ForEach-Object {
        @{
            label = $_.label
            unvm  = $_.unvm; unjit = $_.unjit; compile = $_.compile; build = $_.build; python = $_.python; node = $_.node
            # CSV에도 있는 heap 필드를 JSON에도 넣어 둔다. 없으면 §10 메모리 표를 결과 파일만으로 재생성할 수 없다.
            memNode = $_.memNode; memPython = $_.memPython
            unvmValidation = $_.unvmValid; unjitValidation = $_.unjitValid
            compileValidation = $_.compileValid; buildValidation = $_.buildValid
            pythonValidation = $_.pyValid; nodeValidation = $_.nodeValid
            results = @{ vm=$_.unvmResult; jit=$_.unjitResult; compile=$_.compileResult; exe=$_.buildResult; python=$_.pyResult; node=$_.nodeResult }
            stats = @{ vm=$_.unvmStats; jit=$_.unjitStats; python=$_.pyStats; node=$_.nodeStats }
        }
    }
}

if (-not $NoSave) {
    Set-Content -Path (Join-Path $ResultsDir "results.csv") -Value $Csv -Encoding utf8
    Set-Content -Path (Join-Path $ResultsDir "results.json") -Value ($Json | ConvertTo-Json -Depth 6) -Encoding utf8

    $Md = @()
    $Md += "# 벤치마크 결과 ($Stamp)"
    $Md += ""
    $Md += "## 환경"
    $Md += ""
    foreach ($line in $EnvLines) { $Md += "- $line" }
    $firstBit = BitOf $Rows[0]
    $Md += "- 측정: warmup=$($firstBit.warmup), reps=$($firstBit.reps), 중앙값 비교"
    $Md += ""
    $Md += "| label | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |"
    $Md += "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|"
    foreach ($row in $Rows) {
        $b = BitOf $row
        $rVMPY = RatioOf $row.unvm $row.python
        $rJITN = RatioOf $row.unjit $row.node
        $rJITPY = RatioOf $row.unjit $row.python
        $cells = @(
            $row.label, (ItersOf $row),
            $(if ($row.unvm) { Fmt $row.unvm.median "F4" } else { "n/a" }),
            $(if ($row.unjit) { Fmt $row.unjit.median "F4" } else { "n/a" }),
            $(if ($row.compile) { Fmt $row.compile.median "F4" } else { "n/a" }),
            $(if ($row.build) { Fmt $row.build.median "F4" } else { "n/a" }),
            $(if ($row.python) { Fmt $row.python.median "F4" } else { "n/a" }),
            $(if ($row.node) { Fmt $row.node.median "F4" } else { "n/a" }),
            $(if ($rJITPY) { Fmt $rJITPY "F3" } else { "n/a" }),
            $(if ($rJITN) { Fmt $rJITN "F3" } else { "n/a" })
        )
        $Md += ("| " + ($cells -join " | ") + " |")
    }
    Set-Content -Path (Join-Path $ResultsDir "results.md") -Value $Md -Encoding utf8
    Write-Host "results saved to $ResultsDir"
}

# ---------- 종합 ----------
$jitVsPy = @(); $jitVsNode = @(); $vmVsPy = @(); $vmVsNode = @(); $pyVsNode = @()
foreach ($row in $Rows) {
    $a = RatioOf $row.unjit $row.python;  if ($a) { $jitVsPy += $a }
    $b = RatioOf $row.unjit $row.node;    if ($b) { $jitVsNode += $b }
    $c = RatioOf $row.unvm $row.python;   if ($c) { $vmVsPy += $c }
    $d = RatioOf $row.unvm $row.node;     if ($d) { $vmVsNode += $d }
    $e = RatioOf $row.python $row.node;   if ($e) { $pyVsNode += $e }
}
$geoJitPy  = Geomean $jitVsPy
$geoJitN   = Geomean $jitVsNode
$geoVmPy   = Geomean $vmVsPy
$geoVmN    = Geomean $vmVsNode
$geoPyN    = Geomean $pyVsNode
$jitVsCompile = @(); $jitVsBuild = @()
foreach ($row in $Rows) {
    # RatioOf candidate baseline => baseline/candidate. 라벨이 "JIT / Compile",
    # "JIT / Exe"이므로 candidate에 compile/build를 넣어야 JIT가 분자가 된다.
    $c = RatioOf $row.compile $row.unjit; if ($c) { $jitVsCompile += $c }
    $d = RatioOf $row.build   $row.unjit; if ($d) { $jitVsBuild += $d }
}
$geoJitCompile = Geomean $jitVsCompile
$geoJitBuild   = Geomean $jitVsBuild
Write-Host ""
Write-Host "Geomean ratios (>1 = first language is faster):"
Write-Host ("{0,-12} {1}" -f "Un JIT / Python", $(if ($geoJitPy) { Fmt $geoJitPy "F3" } else { "n/a" }))
Write-Host ("{0,-12} {1}" -f "Un JIT / Node",   $(if ($geoJitN) { Fmt $geoJitN "F3" } else { "n/a" }))
Write-Host ("{0,-12} {1}" -f "Un VM / Python",  $(if ($geoVmPy) { Fmt $geoVmPy "F3" } else { "n/a" }))
Write-Host ("{0,-12} {1}" -f "Un VM / Node",    $(if ($geoVmN) { Fmt $geoVmN "F3" } else { "n/a" }))
Write-Host ("{0,-12} {1}" -f "Python / Node",   $(if ($geoPyN) { Fmt $geoPyN "F3" } else { "n/a" }))
Write-Host ("{0,-12} {1}" -f "JIT / Compile",  $(if ($geoJitCompile) { Fmt $geoJitCompile "F3" } else { "n/a" }))
Write-Host ("{0,-12} {1}" -f "JIT / Exe",      $(if ($geoJitBuild) { Fmt $geoJitBuild "F3" } else { "n/a" }))

# ---------- BENCHMARK.md 재생성 ----------
if ($Report) {
    $categories = @(
        @{ name = "Basic operations"; members = @("int_ops") },
        @{ name = "Control flow";      members = @("nested_sum_200", "sieve30000") },
        @{ name = "Function calls";    members = @("call200000", "fib24") },
        @{ name = "Collections";       members = @("dict5000", "class1000", "rle1994") },
        @{ name = "Algorithms";        members = @("qsort500", "mergesort1000", "matrix48", "math10000", "sin20000", "regex6000", "iter10000") }
    )
    $BenchRow = {
        param($row)
        $sVMPY  = RatioOf $row.unvm $row.python
        $sJITPY = RatioOf $row.unjit $row.python
        $sJITN  = RatioOf $row.unjit $row.node
        @(
            $row.label, (ItersOf $row),
            $(if ($row.unvm) { Fmt $row.unvm.median "F4" } else { "N/A" }),
            $(if ($row.unjit) { Fmt $row.unjit.median "F4" } else { "N/A" }),
            $(if ($row.compile) { Fmt $row.compile.median "F4" } else { "N/A" }),
            $(if ($row.build) { Fmt $row.build.median "F4" } else { "N/A" }),
            $(if ($row.python) { Fmt $row.python.median "F4" } else { "N/A" }),
            $(if ($row.node) { Fmt $row.node.median "F4" } else { "N/A" }),
            $(if ($sJITPY) { Fmt $sJITPY "F3" } else { "N/A" }),
            $(if ($sJITN) { Fmt $sJITN "F3" } else { "N/A" })
        )
    }
    $out = @()
    $out += "# BENCHMARK.md"
    $out += ""
    $out += "Un 벤치마크 결과입니다. 실행: ``scripts\bench.ps1 -Group all -Report`` (Windows)"
    $out += ""
    $out += "## 1. Benchmark Overview"
    $out += ""
    $out += "Un(Rust 바이트코드 컴파일러 + 레지스터 VM + Cranelift JIT)을 CPython 3.13과 Node.js 26과"
    $out += "비교하기 위한 벤치마크다. 각 테스트는 bench/<name>.un, bench/node/<name>.mjs,"
    $out += "bench/python/<name>.py의 세 파일이 **같은 알고리즘, 같은 입력, 같은 검증**을 공유한다."
    $out += ""
    $out += "| 노드 | 내용 |"
    $out += "|---|---|"
    $out += "| 측정 언어 | Un VM (UV_JIT=0), Un JIT(기본), Compile Mode(un native), standalone exe(un build), CPython, Node.js |"
    $out += "| 측정 방식 | 언어 3레인: 워밍업 2회 → reps=7회 / Compile: 자체 타이밍 1회 / Exe: 워밍업 1회 + 7회, Median 비교 |"
    $out += "| 저장 위치 | bench/results/<timestamp>/results.csv, results.json, results.md |"
    $out += "| 보고서 | 이 문서(BENCHMARK.md)은 -Report로 자동 생성된다 |"
    $out += "| 비교 지표 | 중앙값, 종합은 기하평균(∏ rᵢ^(1/n)) |"
    $out += ""
    $out += "## 2. Environment"
    $out += ""
    $out += "- 측정 일시: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz')"
    foreach ($line in $EnvLines) { $out += "- $line" }
    $out += "- 측정 커밋: $commit"
    $out += "- Un 실행 파일: ``rust\target\release\un.exe`` (UV_JIT=0은 VM, 기본은 JIT)"
    $out += ""
    $out += "## 3. Executive Summary"
    $out += ""
    if ($geoJitPy -or $geoJitN -or $geoVmPy -or $geoVmN) {
        if ($geoJitPy) { if ($geoJitPy -ge 1) { $out += "- Un JIT은 Python보다 기하평균 **$(Fmt $geoJitPy 'F3')x 빠르다**." } else { $out += "- Un JIT은 Python보다 기하평균 **$(Fmt (1.0 / $geoJitPy) 'F3')x 느리다**." } }
        if ($geoJitN)  { if ($geoJitN  -ge 1) { $out += "- Un JIT은 Node.js보다 기하평균 **$(Fmt $geoJitN 'F3')x 빠르다**." } else { $out += "- Un JIT은 Node.js보다 기하평균 **$(Fmt (1.0 / $geoJitN) 'F3')x 느리다**." } }
        if ($geoVmPy)  { if ($geoVmPy  -ge 1) { $out += "- Un VM은 Python보다 기하평균 **$(Fmt $geoVmPy 'F3')x 빠르다**." } else { $out += "- Un VM은 Python보다 기하평균 **$(Fmt (1.0 / $geoVmPy) 'F3')x 느리다**." } }
        if ($geoVmN)   { if ($geoVmN   -ge 1) { $out += "- Un VM은 Node.js보다 기하평균 **$(Fmt $geoVmN 'F3')x 빠르다**." } else { $out += "- Un VM은 Node.js보다 기하평균 **$(Fmt (1.0 / $geoVmN) 'F3')x 느리다**." } }
        if ($geoPyN)   { $out += "- Python은 Node.js보다 기하평균 **$(Fmt (1.0 / $geoPyN) 'F3')x 느리다**." }
    }
    $out += "- Un VM은 순수 인터프리터라 Int 루프 구간에서 JIT보다 수백 배 느리다. JIT 범위(IntVec/IntDict/class-method intrinsic)를 벗어나면 VM과 동일하게 인터프리터로 실행되어 두 값은 같아진다."
    $out += ""
    $out += "### 기하평균 요약"
    $out += ""
    $out += "| 지표 (기하평균) | 값 | 해석 |"
    $out += "|---|---|---|"
    if ($geoJitPy) { $out += "| Un JIT / Python | $(Fmt $geoJitPy 'F3')x | >1 = Un이 빠름 |" }
    if ($geoJitN)  { $out += "| Un JIT / Node.js | $(Fmt $geoJitN 'F3')x  | >1 = Un이 빠름 |" }
    if ($geoVmPy)  { $out += "| Un VM / Python | $(Fmt $geoVmPy 'F3')x | >1 = Un이 빠름 |" }
    if ($geoVmN)   { $out += "| Un VM / Node.js  | $(Fmt $geoVmN 'F3')x  | >1 = Un이 빠름 |" }
    if ($geoPyN)   { $out += "| Python / Node.js | $(Fmt $geoPyN 'F3')x  | >1 = Python이 빠름 |" }
    $out += ""
    $out += "## 4. Results (all)"
    $out += ""
    $out += "| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |"
    $out += "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
    foreach ($row in $Rows) {
        $cells = & $BenchRow $row
        $out += ("| " + ($cells -join " | ") + " |")
    }
    $out += ""
    $categoryMap = @(
        @{ name = "5. Basic operations"; members = @("int_ops") },
        @{ name = "6. Control flow";      members = @("nested_sum_200", "sieve30000") },
        @{ name = "7. Function calls";    members = @("call200000", "fib24") },
        @{ name = "8. Collections";       members = @("dict5000", "class1000", "rle1994") },
        @{ name = "9. Algorithms";        members = @("qsort500", "mergesort1000", "matrix48", "math10000", "sin20000", "regex6000", "iter10000") }
    )
    foreach ($cat in $categoryMap) {
        $out += "## $($cat.name)"
        $out += ""
        # §4와 같은 10열 헤더를 쓴다. $BenchRow도 10셀을 내므로 헤더를 8열로 두면 표가 깨진다.
        $out += "| Benchmark | iters | Un VM (ms) | Un JIT (ms) | Native (ms) | Exe (ms) | Python (ms) | Node (ms) | JIT/Python | JIT/Node |"
        $out += "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
        foreach ($member in $cat.members) {
            $r = $Rows | Where-Object { $_.label -eq $member } | Select-Object -First 1
            if ($null -eq $r) { $out += "| $member | N/A | N/A | N/A | N/A | N/A | N/A | N/A | N/A | N/A |"; continue }
            $cells = & $BenchRow $r
            $out += ("| " + ($cells -join " | ") + " |")
        }
        $out += ""
    }
    $out += "## 10. Runtime & Memory"
    $out += ""
    $out += "Un에는 힙 메모리 측정 API가 없어서 VM/JIT의 heap 사용량은 측정하지 못한다. Node와 Python은"
    $out += "인터프리터 heap(heapUsed / tracemalloc)만 측정하며 RSS는 아니다."
    $out += ""
    $out += "| Benchmark | Node heap delta (MB) | Python heap delta (MB) |"
    $out += "|---|---:|---:|"
    foreach ($row in $Rows) {
        $deltaNode = if ($row.memNode) { Fmt $row.memNode.delta "F3" } else { "N/A" }
        $deltaPy = if ($row.memPython) { Fmt $row.memPython.delta "F3" } else { "N/A" }
        $out += "| $($row.label) | $deltaNode | $deltaPy |"
    }
    $out += ""
    $out += "## 11. Concurrency"
    $out += ""
    $out += "- **N/A** — Un의 pool/go/lock은 단일 스레드 협력형 shim이므로(TD-39)"
    $out += "  진정한 병렬성을 측정할 수 없고, 언어별 동시성 모델이 달라 공정할 비교가 성립하지 않는다."
    $out += ""
    $out += "## 12. Overall Comparison"
    $out += ""
    $out += "6레인 전체 결과는 §4 Results (all)과 완전히 같다. 중복 표를 생략하고 §4를 기준으로 사용한다."
    $out += "원시 통계는 bench/results/<timestamp>/results.json, 요약 차트와 CV/95% CI/이상치는"
    $out += "bench/report.html에서 확인할 수 있다."
    $out += ""
    $out += "## 13. Performance Analysis"
    $out += ""
    if ($geoJitPy) {
        if ($geoJitPy -ge 1.0) {
            $out += "- 전반적으로 Un JIT은 Python보다 기하평균 **$(Fmt $geoJitPy 'F3')x 빠르다**."
        } else {
            $out += "- 전반적으로 Un JIT은 Python보다 기하평균 **$(Fmt (1.0 / $geoJitPy) 'F3')배 느리다**."
        }
    }
    if ($geoJitN) {
        if ($geoJitN -ge 1.0) {
            $out += "- 전반적으로 Un JIT은 Node.js보다 기하평균 **$(Fmt $geoJitN 'F3')x 빠르다**."
        } else {
            $out += "- 전반적으로 Un JIT은 Node.js보다 기하평균 **$(Fmt (1.0 / $geoJitN) 'F3')배 느리다**."
        }
    }
    $ratioJitPyOf = {
        param($label)
        $r = $Rows | Where-Object { $_.label -eq $label } | Select-Object -First 1
        if ($null -eq $r) { return $null }
        return RatioOf $r.unjit $r.python
    }
    $ratioJitNodeOf = {
        param($label)
        $r = $Rows | Where-Object { $_.label -eq $label } | Select-Object -First 1
        if ($null -eq $r) { return $null }
        return RatioOf $r.unjit $r.node
    }
    $rIntOpsPy = & $ratioJitPyOf "int_ops"
    $rCallPy   = & $ratioJitPyOf "call200000"
    $rNestedPy = & $ratioJitPyOf "nested_sum_200"
    $rIntOpsN  = & $ratioJitNodeOf "int_ops"
    $rCallN    = & $ratioJitNodeOf "call200000"
    $rNestedN  = & $ratioJitNodeOf "nested_sum_200"
    if ($rIntOpsPy -and $rCallPy -and $rNestedPy) {
        $out += "- Int 루프로 이루어진 구간은 Native로 컴파일된다: int_ops는 Python보다 $(Fmt $rIntOpsPy 'F3')x, call200000은 $(Fmt $rCallPy 'F3')x(Int 함수의 native 직접 호출), nested_sum_200은 $(Fmt $rNestedPy 'F3')x 빠르다."
    }
    if ($rIntOpsN -and $rCallN -and $rNestedN) {
        $out += "- Node.js와 비교해서도 Int 루프 구간의 Un JIT은 강하다: int_ops $(Fmt $rIntOpsN 'F3')x, call200000 $(Fmt $rCallN 'F3')x, nested_sum_200 $(Fmt $rNestedN 'F3')x (1보다 크면 Un이 빠름)."
    }
    $out += "- 아직 인터프리터로 실행되는 구간: 고차 함수·클로저(qsort500), 문자열/정규식, 객체 메서드 다발, math10000/sin20000(math 내재 함수, JIT 범위 밖)."
    $out += "- Node가 강한 지점: 문자열/정규식/리스트 기반 정렬/객체 메서드, V8 JIT으로 웜업된 호출 경로."
    $out += "- Un VM은 순수 인터프리터 실행이라 모든 구간에서 JIT보다 크게 느리다. JIT과 VM의 차이는 위 합성 벤치에서 직접 확인된다."
    $out += ""
    if ($Comparisons -and $Comparisons.Count -gt 0) {
        $out += "## 14. Performance Regression"
        $out += ""
        $out += "측정 편차는 크지 않지만 기계 상태에 좌우된다. 10% 초과 증가만 회귀로 표시한다."
        $prevRel = $PreviousCsv -replace [regex]::Escape($Root + "\"), ""
        $out += "비교 기준: $prevRel (다른 실행)."
        $out += ""
        $out += "| Benchmark | 언어 | 변화 | 판정 |"
        $out += "|---|---|---|---|"
        foreach ($c in $Comparisons) {
            $mark = if ($c.pct -gt 10) { "회귀" } elseif ($c.pct -lt -10) { "개선" } else { "보통" }
            $out += ("| {0} | {1} | {2:+0.0;-0.0}% | {3} |" -f $c.label, $c.lang, $c.pct, $mark)
        }
        $out += ""
        if ($null -ne $PreviousCsv) {
            $prevDir = Split-Path (Split-Path $PreviousCsv -Parent) -Leaf
            $out += "## 15. Historical Comparison"
            $out += ""
            # 이전에는 §14의 ±10% 부분집합을 그대로 복사했다(백틱 `\b`가 U+0008로 새어 나가던 경로도 여기 있었다).
            $out += "이전 결과($prevDir) 대비 10% 초과 변동 항목은 §14의 판정 열(회귀/개선)과 같으므로"
            $out += "중복 표를 생략한다. 전체 추적은 bench/results/<timestamp>/results.csv, 판정 기준은 §14를 참고."
            $out += ""
        }
    }
    $out += "## 16. Benchmark Methodology"
    $out += ""
    $out += "방법론(하네스 프로토콜 BENCH/RESULT/SAMPLE/MEM, warmup/reps, 통계 정의,"
    $out += "회귀 게이트 기준, CSV/JSON 스키마)은 ``docs/guides/BENCHMARK_GUIDE.md``가 단일 기준이다."
    $out += "Compile Mode 재현 노트와 단위 차이는 ``docs/compile/COMPILE_BENCHMARK.md``를 참고."
    $out += ""
    $out += "- 재생성: ``powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench.ps1 -Group all -Report``"
    $out += "- 이 문서는 위 -Report 단일 생성기로만 갱신한다(수동 수정 금지)."
    $out += ""
    $out += "## 17. Known Limitations"
    $out += ""
    $out += "- Un에는 힙 메모리 측정 API가 없다(Node/Python은 tracemalloc/heapUsed 기준 참고치)."
    $out += "- Un의 sys.version()은 ``Un <version>``, os.version()은 ``<OS family> (Un <version>)``를 반환한다(TD-44 해결, 2026-11-19)."
    $out += "- JIT은 Int 부분집합(IntVec/IntDict/class-method intrinsic)만 native 컴파일하고 나머지는 인터프리터로 정확히 실행한다."
    $out += "- Python의 시스템 PATH가 broken WindowsApps stub일 수 있다. 이 러너는 경로를 자동 탐지한다."
    $out += "- Python의 tracemalloc은 Python heap만 잰다. Node의 heapUsed는 V8 heap만 잰다. 둘 다 RSS 비교가 아니다."
    $out += "- 동시성 모델이 다르므로 동시성 벤치는 측정하지 않는다(TD-39)."
    $out += ""
    Set-Content -Path (Join-Path $Root "BENCHMARK.md") -Value ($out -join "`n") -Encoding utf8
    Write-Host "BENCHMARK.md updated"
}

if ($Failures -gt 0) { exit 1 }
exit 0


