<#
.SYNOPSIS
    벤치마크 결과를 HTML 추이 리포트로 변환하고, CI 게이트(회귀 임계)를 검사한다.

.DESCRIPTION
    bench/results/*/results.json 을 읽어 bench/report.html 을 생성한다.
      * 각 벤치의 JIT 중앙값 추이(라인 차트, inline SVG)
      * 최신 실행의 상세 표 + 통계(CV, 95% CI, 이상치)
      * cross-lane 결과 불일치/실패 요약

    -Gate 로 CI 모드 활성화: 최신 실행(timestamp 기준 마지막)을 직전 실행과 비교해
    unjit/compile/build/python/node 중 어느 레인이든 10%(-Threshold) 초과로 느려지면
    exit 1. 실행된 레인의 자기검증 실패도 exit 1. baseline이 없으면 비교를 생략하고
    경고만 출력한다. cross-lane 결과 불일치는 bench.ps1 실행 시 [XLANE]로 집계된다.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench-report.ps1
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench-report.ps1 -Gate -Threshold 10
#>
param([switch]$Gate, [double]$Threshold = 10.0)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$ResultsRoot = Join-Path $Root "bench\results"
$reportPath = Join-Path $Root "bench\report.html"
$runs = @()
if (Test-Path $ResultsRoot) {
    $runs = @(Get-ChildItem $ResultsRoot -Directory | ForEach-Object {
        $j = Join-Path $_.FullName "results.json"
        if (Test-Path $j) {
            try { $data = Get-Content $j -Raw | ConvertFrom-Json; [pscustomobject]@{ Stamp=$data.stamp; Dir=$_.FullName; Data=$data } } catch { }
        }
    } | Sort-Object Stamp)
}
if ($runs.Count -eq 0) { Write-Error "no results.json found in bench/results"; exit 2 }

function Esc([string]$s) { [System.Net.WebUtility]::HtmlEncode($s) }

# ----- JIT 중앙값 추이용 시리즈 -----
$labels = @()
foreach ($r in $runs) { foreach ($row in $r.Data.rows) { if ($row.label -notin $labels) { $labels += $row.label } } }
$series = @()
foreach ($label in $labels) {
    $pts = @()
    foreach ($r in $runs) {
        $row = $r.Data.rows | Where-Object { $_.label -eq $label } | Select-Object -First 1
        if ($row -and $row.unjit -and $row.unjit.median -gt 0) { $pts += [double]$row.unjit.median }
    }
    if ($pts.Count -ge 2) { $series += [pscustomobject]@{ Label=$label; Values=$pts } }
}

function SvgChart($points, $title) {
    $w = 720; $h = 160; $pad = 24
    if ($points.Count -lt 2) { return "<p>$title (not enough data)</p>" }
    $max = ($points | Measure-Object -Maximum).Maximum
    $min = ($points | Measure-Object -Minimum).Minimum
    if ($max -le $min) { $max = $min * 1.1 + 1 }
    $range = $max - $min
    $coords = @()
    for ($i = 0; $i -lt $points.Count; $i++) {
        $x = $pad + ($w - 2*$pad) * $i / ($points.Count - 1)
        $y = $h - $pad - ($h - 2*$pad) * ($points[$i] - $min) / $range
        $coords += "$([math]::Round($x,1)),$([math]::Round($y,1))"
    }
    $line = "<polyline fill='none' stroke='#4f8ff7' stroke-width='2' points='$($coords -join ' ')'/>"
    $dots = ""
    for ($i = 0; $i -lt $coords.Count; $i++) {
        $p = $coords[$i].Split(','); $dots += "<circle cx='$($p[0])' cy='$($p[1])' r='2.5' fill='#4f8ff7'/>"
    }
    return "<div class='chart'><h3>$(Esc $title)</h3><svg viewBox='0 0 $w $h' width='100%' height='$h'><rect width='$w' height='$h' fill='#0f1620'/>$line$dots<text x='$pad' y='14' fill='#9fb3c8' font-size='11'>max=$([math]::Round($max,3))</text><text x='$pad' y='$($h-6)' fill='#9fb3c8' font-size='11'>min=$([math]::Round($min,3))</text></svg></div>"
}

$html = @()
$html += "<!doctype html><html lang='ko'><head><meta charset='utf-8'><title>Un Benchmark Report</title>"
$html += "<style>body{background:#0b1017;color:#dce6f0;font-family:system-ui,Segoe UI,sans-serif;margin:24px}h1,h2{color:#8fc0ff}table{border-collapse:collapse;width:100%;font-size:13px}th,td{border:1px solid #2a3a4d;padding:5px 8px;text-align:right}th{background:#162232;color:#9fc8ff;text-align:center}td.l,th.l{text-align:left}.bad{color:#ff8080}.good{color:#7ee0a0}.chart{margin:18px 0}svg{border:1px solid #2a3a4d;border-radius:6px}</style></head><body>"
$html += "<h1>Un Benchmark Report</h1>"
$html += "<p>Generated: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz') · Runs: $($runs.Count)</p>"

$latest = $runs[-1]
$html += "<h2>Latest: $(Esc $latest.Stamp) (commit $(Esc $latest.Data.commit))</h2>"
$html += "<table><tr><th class='l'>Benchmark</th><th>VM (ms)</th><th>JIT (ms)</th><th>Python</th><th>Node</th><th>CV (JIT)</th><th>95% CI (JIT)</th><th>Outliers</th></tr>"
foreach ($row in $latest.Data.rows) {
    $st = $row.stats.jit
    $ci = if ($st) { "$([math]::Round($st.ci95Lo,3))–$([math]::Round($st.ci95Hi,3))" } else { "-" }
    $cv = if ($st) { [math]::Round($st.cv,3) } else { "-" }
    $out = if ($st) { $st.outliers } else { "-" }
    $html += "<tr><td class='l'>$(Esc $row.label)</td><td>$(if($row.unvm){[math]::Round($row.unvm.median,3)}else{'n/a'})</td><td>$(if($row.unjit){[math]::Round($row.unjit.median,3)}else{'n/a'})</td><td>$(if($row.python){[math]::Round($row.python.median,3)}else{'n/a'})</td><td>$(if($row.node){[math]::Round($row.node.median,3)}else{'n/a'})</td><td>$cv</td><td>$ci</td><td>$out</td></tr>"
}
$html += "</table>"

$html += "<h2>JIT median trends</h2>"
foreach ($s in $series) { $html += SvgChart $s.Values $s.Label }

$html += "</body></html>"
[IO.File]::WriteAllText($reportPath, ($html -join "`n"), (New-Object Text.UTF8Encoding($true)))
Write-Host "report written to $reportPath"

# ----- CI gate -----
if ($Gate) {
    $problems = @()
    if ($runs.Count -lt 2) {
        Write-Host "GATE: only one run available; regression comparison skipped" -ForegroundColor Yellow
    } else {
        $prevModes = @($runs[-2].Data.modes)
        $curModes = @($latest.Data.modes)
        if (($prevModes -join ',') -ne ($curModes -join ',')) {
            Write-Host "GATE: baseline modes ($($prevModes -join ',')) != latest modes ($($curModes -join ',')); ratios may not be comparable" -ForegroundColor Yellow
        }
    }
    foreach ($row in $latest.Data.rows) {
        foreach ($lang in @("unjit","compile","build","python","node")) {
            $cur = $row.$lang
            if (-not $cur -or -not $cur.median) { continue }
            $prev = $null
            if ($runs.Count -ge 2) {
                $pr = $runs[-2].Data.rows | Where-Object { $_.label -eq $row.label } | Select-Object -First 1
                if ($pr -and $pr.$lang) { $prev = [double]$pr.$lang.median }
            }
            if ($prev -and $prev -gt 0) {
                $pct = ([double]$cur.median - $prev) / $prev * 100.0
                if ($pct -gt $Threshold) { $problems += "$($row.label)/$lang +$([math]::Round($pct,1))%" }
            }
        }
        # 실행된 레인의 자기검증 실패는 모두 게이트 실패다. "skipped" /
        # "compile-rejected (skipped)" / "self-timed (no asserts)" / "n/a (exe)" 는 정상.
        $validationFields = @(
            @{ mode = "unvm";   field = "unvmValidation" },
            @{ mode = "unjit";  field = "unjitValidation" },
            @{ mode = "python"; field = "pythonValidation" },
            @{ mode = "node";   field = "nodeValidation" }
        )
        foreach ($v in $validationFields) {
            if ($latest.Data.modes -notcontains $v.mode) { continue }
            $val = [string]$row.($v.field)
            if ($val -match "/ [1-9][0-9]* failed" -or $val -eq "none") {
                $problems += "$($row.label)/$($v.mode) validation failed ($val)"
            }
        }
        if ($latest.Data.modes -contains "unnative" -and [string]$row.compileValidation -eq "failed") { $problems += "$($row.label)/compile failed" }
        if ($latest.Data.modes -contains "unbuild" -and [string]$row.buildValidation -eq "failed") { $problems += "$($row.label)/build failed" }
    }
    if ($problems.Count -gt 0) {
        Write-Host "GATE FAILED: $($problems.Count) problem(s): $($problems -join ', ')" -ForegroundColor Red
        exit 1
    }
    Write-Host "GATE OK"
}
exit 0
