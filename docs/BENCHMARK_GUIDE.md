# Benchmark Guide

Benchmarks live in `bench/`. Each measurement is a script that validates its
own result with assertions and then prints statistics for one workload. There
is no benchmark code inside `test/`.

All four language slots implement the *same algorithm on the same input with
the same expected values*, and are self-validating before they measure:

- Un: `bench/<name>.un` — validated with `test/assert.un`, prints `N passed, M failed`
- Node.js: `bench/node/<name>.mjs` — validated with `node:assert/strict`, prints `=== Verify: ok ===`
- Python: `bench/python/<name>.py` — validated with plain `assert`, prints `=== Verify: ok ===`

## Running benchmarks

```powershell
# All lanes (Un VM + Un JIT + Python + Node), save under bench/results/<timestamp>/
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bench.ps1

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
```

A single benchmark can be run directly:

```powershell
.\rust\target\release\unvm.exe run bench\sort.un
.\rust\target\release\unvm.exe run bench\sort.un            # Un JIT (default)
$env:UV_JIT = "0"; .\rust\target\release\unvm.exe run bench\sort.un  # Un VM only
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

Input generation is identical across the three languages:
`s = (s * 1103515245 + 12345) % 100000`.

## Measurement rules

All three harnesses follow the same rules, so numbers are comparable:

1. **Warmup**: `warmup = 2` runs are executed before timing (JIT/interpreter
   cache, allocation pools, string interning).
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

## What is not measured

- **Memory for Un**: Un has no API to read heap usage, so the memory columns are
  available only for Node (`process.memoryUsage().heapUsed`) and Python
  (`tracemalloc`). These measure interpreter-managed heap, not RSS.
- **Process startup**: each script is a fresh process; the timer covers steady
  state, and absolute times include startup overhead in each language equally.
- **GC/JIT internals**: not available from Un.
- **Multithreading**: `pool`/`go` are single-threaded cooperative shims today
  (see TD-39), so they are not benchmarked.
- **`%` and `//` in JIT**: integer div/mod are JIT-eligible only with a constant
  integer divisor; `i % 4 == 0` in a tight loop still measures interpreter work
  unless the divisor is constant.

## Comparing languages

Rules:

- Only tools actually installed and runnable on this machine are used: Un
  (this repository, release `unvm.exe`), Node.js v26.13.0 (`v26.x` ships
  V8 14), and Python 3.13.17 (CPython, verified working on this machine).
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

## Notes on stability

- Un medians for the same script vary by a few percent between runs; Node
  medians for the small workloads vary by more (e.g. `nested_sum_200` at
  0.027 ms in one run and 0.187 ms in another).
- Python's numbers are extremely sensitive to CPU frequency and especially to
  `.pyc` caching; prefer the median, and treat < 10% differences as noise.
- A recorded result directory is a record of one run, not a portable constant.
