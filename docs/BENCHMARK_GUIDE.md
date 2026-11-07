# Benchmark Guide

Benchmarks live in `bench/`. Each measurement is a script that validates its
own result with assertions and then prints statistics for one workload. There
is no benchmark code inside `test/`.

## Running benchmarks

```powershell
# Un + Node, results saved under bench/results/<timestamp>/
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1

# Un only
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group un

# Node only
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group node

# Rebuild first, print only (no saved results)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Rebuild -NoSave
```

A single benchmark can be run directly:

```powershell
$env:DOTNET_ROLL_FORWARD = "Major"
.\bin\Release\net8.0\Un.exe run bench\sort.un
node bench\node\sort.mjs
```

## What is measured

Every pair (`bench/<name>.un`, `bench/node/<name>.mjs`) implements the same
algorithm on the same input, with the same expected values:

| Label | Workload | iters |
|---|---|---|
| `fib24` | naive recursive fib(24) | 1 call |
| `nested_sum_200` | i*j double loop, 200x200 | 40000 mul/add |
| `qsort500` | quicksort on 500 LCG numbers | 500 elements |
| `rle1994` | RLE encode + decode of 1994 characters | 1994 characters |
| `dict5000` | 5000 inserts + 5000 lookups | 10000 operations |
| `iter10000` | range + map + filter + sum on 10000 values | 30000 operations |
| `math10000` | floor(sqrt(i)) sum, i = 1..10000 | 10000 evaluations |
| `class1000` | 1000 instances + 1000 method calls | 1000 instances |

Input generation is identical in both languages: the LCG
`s = (s * 1103515245 + 12345) % 100000` produces the same sequence in Un int64
and Node doubles for the values used here.

## Measurement rules

Both harnesses follow the same rules, so numbers are comparable:

1. **Warmup**: `warmup = 2` runs are executed before timing (JIT/interpreter
   cache, allocation pools, string interning).
2. **Repeated runs**: `reps = 7` timed runs. Each run is timed individually, so
   the distribution is visible.
3. **No output inside the timed region**: the harness starts the timer, runs the
   workload, stops the timer, and only then writes lines. `assert.summary()` and
   the verification lines are printed outside the timed region.
4. **Statistics**: Min, Median, Average, Max, population StdDev of the run times,
   and OPS computed from the median (`iters / (median_ms / 1000)`).
5. **Median definition**: the element at index `floor(n/2)` of the sorted times
   (Un `times[n // 2]`, Node `times[Math.floor(n/2)]`). Both harnesses use the
   same definition, so medians are directly comparable.
6. **Timer resolution**: Un `time.stopwatch().tick()` is an int at 0.1 ms
   resolution, so Un times are recorded as `tick / 10000.0` ms. `sw.ms()`
   (integer ms) is too coarse for workloads under 10 ms. Node uses
   `performance.now()`.
7. **Environment is recorded**: `bench/env.un` and `bench/node/env.mjs` print
   `ENV|` lines (OS, platform, arch, CPU count, recursion limit, CPU model), and
   the runner stores them with the results.
8. **Validation is mandatory**: each script asserts its own expected values
   before measuring. If an assertion fails, the run is not a valid benchmark
   result, and the runner reports `[BAD]` and exits non-zero.

The runner pairs results by label, prints a table, computes the ratio
`Un median / Node median`, and saves `results.csv` + `results.md` under
`bench/results/<timestamp>/`.

## What is not measured

- **Memory on the Un side**: Un has no API to read heap usage or GC counters, so
  the memory column is `n/a` for Un. Node reports `MEM|label|delta MB|heap MB`
  (`process.memoryUsage().heapUsed`), which is one-sided; GC timing is not
  controlled, so it is a reference number, not a comparison metric.
- **Process startup**: each script is launched as a fresh process, so the reported
  time excludes interpreter startup and module loading. This is a fair comparison
  for steady-state workloads, not for short scripts.
- **GC/JIT internal cost breakdown**: not available from Un.
- **Multithreading**: `pool`/`go` is deliberately not benchmarked, because
  worker pools hang at process exit when they are not under `using` and
  `close()`-then-`map` crashes with an uncaught exception. Measuring them would
  require fixing the implementation first.

## Comparing languages

The rules used here:

- Only environments that are installed and runnable on this machine are used:
  Un (this repository) and Node.js v26.13.0. Python is a broken WindowsApps
  stub on this machine (exit 9009), so it is not used at all.
- Same algorithm, same input, same expected values, validated in each script.
- Same warmup, same number of repeats, same median definition.
- Environment recorded with the results.
- No bias toward Un: the Node workloads are the same size as the Un workloads.
  The ratio is reported as `Un/Node`, which means Un is that many times slower.
  The ratio is an interpreter-vs-JIT comparison, not a language quality verdict.

Rough observed ratios for 2026-11-06 (see `docs/BENCHMARK_LOG.md` for the table):
`1000x` for fib, `90-900x` for the loop/sort/string/math/class workloads,
`35-390x` for the collection/iterator workloads.

## Adding a benchmark

1. Write `bench/<name>.un`: validate with `test/assert.un`, then
   `bench.harness.measure("label", fn(n) -> <workload>, iters, 7, 2)`.
2. Write `bench/node/<name>.mjs` with the same algorithm, input, expected
   values, and `measure("label", (n) => ..., iters, 7, 2)` from
   `bench/node/harness.mjs`, printing `=== Verify: ok ===` at the end.
3. Add the pair (`label`, un path, node path) to `$Pairs` in `scripts/bench.ps1`.
4. Run `scripts/bench.ps1` and check that both validations pass and the ratio
   is stable across a few runs.
5. Keep the workload inside measurable items only: no unbounded recursion, no
   pool/`go`, no reading from stdin, no sleeps.

## Notes on stability

- Un medians for the same script vary by a few percent between runs; Node medians
  for the small workloads vary by more than that (the Node median for
  `nested_sum_200` was 0.027 ms in one run and 0.187 ms in another), so use the
  median-of-medians over several runs when reporting a ratio.
- Numbers are machine-specific. A recorded result directory is a record of one
  run, not a portable constant.
