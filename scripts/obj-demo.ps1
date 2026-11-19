<#
.SYNOPSIS
    Phase 1 demo for `un obj`: compile a small Un program to an object file
    from its .unbc and link it into a standalone .exe using rustc's bundled
    mingw gcc driver.
.DESCRIPTION
    실행: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\obj-demo.ps1
    단계: un compile (fib.un → .unbc) → un obj (.unbc → .obj) → rustc link
    (bench\obj_fib_driver.rs + .obj → obj_fib_demo.exe) → 실행 (출력은 `fib = 55`).
#>
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$Exe = Join-Path $Root "rust\target\release\un.exe"
$Driver = Join-Path $Root "bench\obj_fib_driver.rs"
$Tmp = Join-Path $env:TEMP ("un_obj_demo_" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $Tmp -Force | Out-Null

# Demo source: module proto invokes fib; `main` is the entry proto.
$Source = Join-Path $Tmp "fib.un"
Set-Content -Path $Source -Value @'
use io

fn fib(n: int) -> int
    if n < 2
        -> n
    -> fib(n - 1) + fib(n - 2)

fn main() -> int
    -> fib(10)

io.write(main())
'@ -Encoding ascii

Copy-Item $Driver (Join-Path $Tmp "driver.rs")

Push-Location $Tmp
try {
    & $Exe compile fib.un
    if ($LASTEXITCODE -ne 0) { exit 1 }
    & $Exe obj fib.un.unbc -o fib.obj
    if ($LASTEXITCODE -ne 0) { exit 1 }

    $gcc = Join-Path $env:USERPROFILE ".rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained\x86_64-w64-mingw32-gcc.exe"
    if (-not (Test-Path $gcc)) {
        Write-Host "mingw gcc (rustup self-contained) not found at $gcc" -ForegroundColor Red
        exit 1
    }
    rustup run stable-x86_64-pc-windows-gnu rustc --edition 2021 driver.rs -C linker="$gcc" -C link-arg=fib.obj -o obj_fib_demo.exe
    if ($LASTEXITCODE -ne 0) { exit 1 }

    $out = & .\obj_fib_demo.exe
    Write-Host "obj-demo: $out"
    if ("$out" -ne "fib = 55") {
        Write-Host "obj-demo: FAILED (expected 'fib = 55')" -ForegroundColor Red
        exit 1
    }

    # un build: one-command native executable from the build-demo source.
    # This program prints via io.write (exercises the native runtime bridge).
    $BuildSrc = Join-Path $Tmp "build_fib.un"
    Set-Content -Path $BuildSrc -Value @'
use io

fn fib(n: int) -> int
    if n < 2
        -> n
    -> fib(n - 1) + fib(n - 2)

fn main() -> int
    io.write(fib(10))
    -> 0
'@ -Encoding ascii
    & $Exe build $BuildSrc -o obj_fib_build.exe
    if ($LASTEXITCODE -ne 0) { exit 1 }
    $out2 = (& .\obj_fib_build.exe).Trim()
    Write-Host "obj-demo (un build): $out2"
    if ("$out2" -ne "55") {
        Write-Host "obj-demo (un build): FAILED (expected '55')" -ForegroundColor Red
        exit 1
    }

    # un build --embed: same source, via the embedded VM runtime — a complete
    # standalone executable (all language features, not just the Int core).
    $embedSrc = Join-Path $Tmp "embed_fib.un"
    Set-Content -Path $embedSrc -Value @'
use io

fn fib(n) -> int
    if n < 2
        -> n
    -> fib(n - 1) + fib(n - 2)

io.write(fib(10))
'@ -Encoding ascii
    & $Exe build --embed $embedSrc -o embed_fib.exe
    if ($LASTEXITCODE -ne 0) { exit 1 }
    $out3 = & .\embed_fib.exe
    Write-Host "obj-demo (un build --embed): $out3"
    if ("$out3" -ne "55") {
        Write-Host "obj-demo (un build --embed): FAILED (expected '55')" -ForegroundColor Red
        exit 1
    }

    # un build --embed with a class: standalone exe for any program.
    $classSrc = Join-Path $Tmp "embed_class.un"
    Set-Content -Path $classSrc -Value @'
use io

class Point(x: int, y: int)
    fn total() -> int
        -> self.x + self.y

fn main() -> int
    p = Point(3, 4)
    q = Point(10, 20)
    io.write(p.total() + q.total())
    -> 0

main()
'@ -Encoding ascii
    & $Exe build --embed $classSrc -o embed_class.exe
    if ($LASTEXITCODE -ne 0) { exit 1 }
    $out4 = & .\embed_class.exe
    Write-Host "obj-demo (un build --embed class): $out4"
    if ("$out4" -ne "37") {
        Write-Host "obj-demo (un build --embed class): FAILED (expected '37')" -ForegroundColor Red
        exit 1
    }

    # Plain `un obj`/`un build` on a class program: classes are supported
    # natively; the exe must produce the same output as `un run`.
    $prevEAP = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & $Exe build $classSrc -o embed_class_plain.exe *> $null
    $plainCode = $LASTEXITCODE
    if ($plainCode -ne 0) {
        $ErrorActionPreference = $prevEAP
        Write-Host "obj-demo (un build class): FAILED (expected exit 0, got $plainCode)" -ForegroundColor Red
        exit 1
    }
    $out5 = & .\embed_class_plain.exe 2>&1 | Out-String
    $ErrorActionPreference = $prevEAP
    $out5Trim = $out5.Trim()
    Write-Host "obj-demo (un build class): $out5Trim"
    if ($out5Trim -ne "37") {
        Write-Host "obj-demo (un build class): FAILED (expected '37')" -ForegroundColor Red
        exit 1
    }

    # Plain `un build` on a program whose core functions use class-typed
    # lists: the exe must produce the same output as `un run`.
    $classListSrc = Join-Path $Tmp "class_list.un"
    Set-Content -Path $classListSrc -Value @'
use io

class Point(x: int, y: int)
    fn total() -> int
        -> self.x + self.y

fn sum_points(pts: list[Point]) -> int
    total = 0
    for p in pts
        total += p.total()
    -> total

fn main() -> int
    xs = [Point(1, 2), Point(10, 20)]
    xs[0].x = 5
    t = 0
    for p in xs
        t = t + p.total()
    io.write(t)
    xs.add(Point(100, 200))
    io.write(sum_points(xs))
    io.write(len(xs))
    -> 0

main()
'@ -Encoding ascii
    $runOut = & $Exe run $classListSrc
    if ($LASTEXITCODE -ne 0) {
        Write-Host "obj-demo (un build class list): FAILED (un run exited $LASTEXITCODE)" -ForegroundColor Red
        exit 1
    }
    & $Exe build $classListSrc -o class_list_plain.exe *> $null
    $plainCode = $LASTEXITCODE
    if ($plainCode -ne 0) {
        Write-Host "obj-demo (un build class list): FAILED (expected exit 0, got $plainCode)" -ForegroundColor Red
        exit 1
    }
    $out6 = (& .\class_list_plain.exe) -join "`n"
    $run6 = $runOut -join "`n"
    Write-Host "obj-demo (un build class list): $out6"
    if ($out6 -ne $run6) {
        Write-Host "obj-demo (un build class list): FAILED (un run printed '$run6')" -ForegroundColor Red
        exit 1
    }

    Write-Host "obj-demo: PASS" -ForegroundColor Green
    exit 0
} finally {
    Pop-Location
    Remove-Item $Tmp -Recurse -Force -ErrorAction SilentlyContinue
}
