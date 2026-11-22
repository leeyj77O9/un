# Benchmark Guide

Benchmarks live in `bench/`. Each measurement is a script that validates its
own result with assertions and then prints statistics for one workload. There
is no benchmark code inside `test/`.

All three language harnesses implement the *same algorithm on the same input with
the same expected values*, and are self-validating before they measure. The
`bench.ps1 -Group all` runner measures six lanes: Un VM, Un JIT, Compile Mode,
standalone exe, Python, and Node.js:

- Un: `bench/<name>.un` — validated with `test/assert.un`, prints `N passed, M failed`
- Node.js: `bench/node/<name>.mjs` — validated with `node:assert/strict`, prints `=== Verify: ok ===`
- Python: `bench/python/<name>.py` — validated with plain `assert`, prints `=== Verify: ok ===`

Two extra compile-mode lanes are measured for the typed self-timed fixtures
(`bench/native_<name>.un`): `un native` (Compile Mode) and a standalone exe
built with `un build`. Those fixtures print bare millisecond values instead of
`BENCH|` lines and skip assertion validation; dynamic/untyped workloads show
`N/A` in the compile columns (they are outside the compile-mode subset).
Three workloads still have no compile variant: `rle1994` (character indexing),
`matrix48` (nested lists) and `regex6000` (match objects).
`dict5000` uses `codes.hex(...)` string keys because core dicts are `dict[str, int]`;
its compile numbers include key-conversion overhead.

The table below lists the 24 independent triplets. `bench.ps1` reports one extra
row, `native_fib24`: it is the annotated `bench/native_fib.un` fixture (shared with
`scripts/compile-bench.ps1`) and reuses the `fib24` Node/Python scripts, so it is
not a 25th independent workload.

## Running benchmarks

```powershell
# All lanes (Un VM + Un JIT + Compile Mode + standalone exe + Python + Node), save under bench/results/<timestamp>/
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1

# Compile-mode lanes only (un native + plain un build exe)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group native
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group unnative
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group unbuild

# One or more lanes (comma-separated combos possible)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group unjit
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group unvm,python
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group python
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group node
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group all -Report   # also regenerate BENCHMARK.md

# Only selected benchmarks (names = the labels in the table below)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group all -Only qsort500,mergesort1000

# Compare against a previous results.csv
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1 -Group all -Compare bench\results\20261116-235527\results.csv

# Edge-case fuzz: same inputs across VM/JIT/Compile/Python/Node, byte-compare RESULT|case|value
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\edge-fuzz.ps1

# HTML trend report + CI gate (fails on >10% regression or validation errors)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench-report.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench-report.ps1 -Gate -Threshold 10
```

`BENCHMARK.md` has exactly one generator: `bench.ps1 -Report`. Do not hand-edit it;
fix the generator and regenerate instead. `bench.ps1 -Compare` is advisory — it
prints >10% changes but does not increment the failure count or change the exit
code. The pass/fail regression gate is `bench-report.ps1 -Gate` (below).

A single benchmark can be run directly:

```powershell
.\rust\target\release\un.exe run bench\sort.un
.\rust\target\release\un.exe run bench\sort.un            # Un JIT (default)
$env:UV_JIT = "0"; .\rust\target\release\un.exe run bench\sort.un  # Un VM only
node bench\node\sort.mjs
python bench\python\sort.py
```

## What is measured

Every triplet (`bench/<name>.un`, `bench/node/<name>.mjs`, `bench/python/<name>.py`)
implements the same algorithm on the same input with the same expected values:

| Label | Category | Workload | iters |
|---|---|---|---|
| `int_ops` | Basic int ops | LCG-ish int arithmetic + modulo + bitwise + xor per iter | 1_000_000 |
| `fib24` | Function calls / recursion | naive recursive fib(24) | 1 call |
| `native_fib24` | Compile fixture | annotated fib(24) (`bench/native_fib.un`), reuses `fib24` Node/Python | 1 call |
| `nested_sum_200` | Control flow | i*j double loop, 200x200 | 40000 mul/add |
| `qsort500` | Algorithms | quicksort on 500 LCG numbers | 500 elements |
| `mergesort1000` | Algorithms | mergesort on 1000 LCG numbers | 1000 elements |
| `rle1994` | Collections / strings | RLE encode + decode of 1994 characters | 1994 characters |
| `dict5000` | Collections | 5000 inserts + 5000 lookups | 10000 operations |
| `iter10000` | Algorithms | range + map + filter + sum on 10000 values | 30000 operations |
| `math10000` | Misc | floor(sqrt(i)) sum, i = 1..10000 | 10000 evaluations |
| `class1000` | Collections / classes | 1000 instances + 1000 method calls | 1000 instances |
| `sieve30000` | Control flow | sieve of Eratosthenes, primes below 30000 | 30000 slots |
| `matrix48` | Algorithms | 48x48 integer matrix multiply + checksum | 110592 mul/add |
| `sin20000` | Misc | `math.sin(i)` sum for i = 1..20000 | 20000 evaluations |
| `call200000` | Function calls | `mix()` LCG chain of 200000 calls (call overhead) | 200000 calls |
| `regex6000` | Algorithms | `re.find_all("[0-9]+")` on a 53813-char text (6000 tokens) | 6000 matches |
| `str_unicode` | Strings | Korean/emoji string build + upper + ASCII slice | 2000 iterations |
| `except10000` | Control flow | conditional branch over 10000 values | 10000 iterations |
| `match20000` | Pattern matching | `match x % 4` over 20000 values | 20000 iterations |
| `closure20000` | Closures | 10 capturing closures called 20000 times | 20000 calls |
| `inherit1000` | Classes | 1000 derived-class instances + method dispatch | 1000 instances |
| `set5000` | Collections | set add + contains over 5000 values | 5000 operations |
| `enum10000` | Enum | enum value dispatch over 10000 values | 10000 iterations |
| `modimport2000` | Modules | cross-function call chain over 2000 values | 2000 iterations |
| `nanbox20000` | Float | mixed float/int arithmetic over 20000 values | 20000 iterations |

Input generation is identical across the three languages:
`s = (s * 1103515245 + 12345) % 100000`.

## Measurement rules

All three harnesses follow the same rules, so numbers are comparable:

1. **Warmup**: `warmup = 2` runs are executed before timing (JIT/interpreter
   cache, allocation pools, string interning). This applies to the three language
   harnesses; the extra lanes differ — `un native` self-timed fixtures contribute
   one value per run (`reps=1` in the artifacts), and the standalone-exe lane uses
   1 warmup run plus 7 process runs. Do not read every column as a 7-sample median.
2. **Repeated runs**: `reps = 7` timed runs. Each run is timed individually,
   so the distribution is visible (min/median/avg/max/stddev).
3. **No output inside the timed region**: the harness starts the timer,
   runs the workload, stops the timer, and only then writes the `BENCH|…`
   line. `assert.summary()` / the verify line are printed outside it.
4. **Statistics**: Min, Median, Average, Max, population StdDev of the run
   times, and OPS computed from the median (`iters / (median_ms / 1000)`).
5. **Median definition**: the element at index `floor(n//2)` of the sorted
   times. All three harnesses use the same definition.
6. **Timer resolution**: Un uses `time.stopwatch().tick()` (100 ns, 10^7 ticks/s),
   recorded as `tick / 10000.0` ms. Node uses `performance.now()`. Python uses
   `time.perf_counter()`.
7. **Environment is recorded**: `bench/env.un` / `bench/node/env.mjs` /
   `bench/python/env.py` print the `ENV|` lines (OS, platform, arch, CPU count,
   model) and the runner adds `ENV|un_commit|<sha>`.
8. **Validation is mandatory**: each script asserts its own expected values
   before measuring; the runner reports `[BAD]` and exits non-zero on failure.
9. **Primary comparison metric is the median**; the executive summary reports
   the geomean of the ratio `baseline / candidate` over all validated rows.
10. **Ratios**: `X / Y` means `Y_median / X_median`. `> 1.00` = X is faster
    than Y. `pi / node` would be Python vs Node.

The runner pairs results by label, prints a table, and saves `results.csv`,
`results.json` and `results.md` under `bench/results/<timestamp>/`.

### Saved artifacts (schema)

- `results.csv`: one row per benchmark — `label,iters,reps,warmup`, then six fields
  per lane (`<lane>_min_ms`, `_median_ms`, `_avg_ms`, `_max_ms`, `_stddev_ms`, `_ops`)
  for `unvm,unjit,compile,build,python,node`, then `python_heap_delta_mb`,
  `python_heap_mb`, `node_heap_delta_mb`, then six `*_validation` columns.
  The regression reader expects the `*_median_ms` column names (older
  `un_median_ms` is accepted as a fallback for `unjit_median_ms`).
- `results.json`: `stamp`, `commit`, `args`, `modes`, `env[]` and `rows[]`. Each row
  holds the per-lane objects, `results` (canonical `RESULT|` values), `stats`
  (mean, sd, CV, p95, Grubbs outliers, 95% CI), the six validation strings and
  `memNode` / `memPython` (the latter are written by new runs; older JSONs from
  before this field was added lack them).
- `results.md`: the compact median table; it is a record of that run, not a baseline.

### Regression gate (`bench-report.ps1 -Gate`)

- Baseline is the second-newest `bench/results/*/results.json` by `stamp`; the
  candidate is the newest. Each benchmark/lane pair present in both is compared.
- Checked lanes: `unjit`, `compile`, `build`, `python`, `node`. `unvm` is reported
  in the tables but is not a gate lane.
- Failure: a lane median more than `-Threshold` percent slower than the baseline
  (default 10). Improvements never fail. Exit codes: 0 = OK, 1 = failure,
  2 = no `results.json` at all.
- Self-validation failures on lanes that actually ran also fail the gate
  (Un/Node/Python validation strings, plus compile/build `failed` markers).
  `skipped`, `compile-rejected (skipped)` and `self-timed (no asserts)` are normal.
- With a single run the comparison is skipped with a warning. If the two runs
  used different lane sets, a comparability warning is printed; compare results
  from the same `-Group`/`-Only` set for meaningful numbers.
- Cross-lane `RESULT|` mismatches are caught earlier by `bench.ps1` (`[XLANE]`)
  and by `scripts/edge-fuzz.ps1`, not by this gate.

### Edge cases

The seven `bench/edge/*.un` cases (`big_input`, `div_zero`, `empty_collections`,
`float_edge`, `int_boundary`, `nested_collections`, `unicode_edge`) are
correctness-only cross-lane fuzz inputs for `scripts/edge-fuzz.ps1`; they are not
part of the timing tables. `unicode_edge` RESULT comparison is intentionally
skipped because `len(emoji)` differs (Un/Node count UTF-16 code units, Python
counts code points).

## What is not measured

- **Memory for Un**: Un has no API to read heap usage, so the memory columns are
  available only for Node (`process.memoryUsage().heapUsed`) and Python
  (`tracemalloc`). These measure interpreter-managed heap, not RSS.
- **Process startup**: each script is a fresh process; the timer covers steady
  state, and absolute times include startup overhead in each language equally.
- **GC/JIT internals**: not available from Un.
- **Multithreading**: `pool`/`go` remain single-threaded cooperative shims by
  design (`DECISIONS.md` D-054/D-061; TD-39 was resolved 2026-11-19), so they
  are not benchmarked.
- **`%` and `//` in JIT**: integer div/mod are JIT-eligible only with a constant
  integer divisor; `i % 4 == 0` in a tight loop still measures interpreter work
  unless the divisor is constant.

## Comparing languages

Rules:

- Only tools actually installed and runnable on this machine are used: Un
  (this repository, release `un.exe`), Node.js and CPython. Exact versions are
  recorded per run in the `ENV|node` / `ENV|python` lines; version strings in
  older reports are historical observations, not baselines.
- Same algorithm, same input, same expected values, validated in each script.
- Same warmup, same number of repeats, same median definition.
- Environment recorded with the results.
- Ratios are reported as `X / Y = Y_median / X_median`.

## Adding a benchmark

1. Write `bench/<name>.un`: validate with `test/assert.un`, then
   `bench.harness.measure("label", fn(n) -> <workload>, iters, 7, 2)`.
2. Write `bench/node/<name>.mjs` (same algorithm/input/verification) with
   `measure("label", ..., iters, 7, 2)` from `bench/node/harness.mjs`.
3. Write `bench/python/<name>.py` (same) with `measure("label", ..., iters, 7, 2)`
   from `bench/python/harness.py`.
4. Add the pair (`label`, un, node, python paths) to `$Pairs` in `scripts/bench.ps1`.
5. Run `scripts/bench.ps1` and confirm all validations pass.
6. Keep the workload within measurable items; no sleeps, no stdin, no pools.

## Compile-mode reproduction

`scripts/compile-bench.ps1` is a focused, independent check of `bench/native_fib.un`
across `un run` (VM+JIT), `un run` with `UV_JIT=0`, `un native`, and a plain
`un build` executable. Both scripts currently use the same self-timed fixture
(`bench/native_fib_exe.un`: 700 in-process `fib(24)` calls, ms per call) and a
median of 7 process runs, so their fib rows share a unit. They are still separate
runs on different dates/machine states — never merge their numbers into one
table or treat an older figure as the current baseline. Historical compile-mode
measurements and their caveats are in `docs/compile/COMPILE_BENCHMARK.md`; this
guide owns the method, and `BENCHMARK.md` owns the current numbers.

## Notes on stability

- Un medians for the same script vary by a few percent between runs; Node
  medians for the small workloads vary by more (e.g. `nested_sum_200` at
  0.027 ms in one run and 0.187 ms in another).
- Python's numbers are extremely sensitive to CPU frequency and especially to
  `.pyc` caching; prefer the median, and treat < 10% differences as noise.
- A recorded result directory is a record of one run, not a portable constant.
