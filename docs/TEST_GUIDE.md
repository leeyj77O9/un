# Test Guide

This document explains how the Un test suite is organized and how to run it.
There is no unit-test runner in a build system: every test is an `.un` script
executed by the interpreter, and the results are aggregated by a PowerShell runner.

## Running the tests

Run everything from the repository root (`C:\dev\un`):

```powershell
# Rebuild, then run the whole suite
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test.ps1 -Rebuild

# One group
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test.ps1 -Group unit
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test.ps1 -Group integration
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test.ps1 -Group negative
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test.ps1 -Group known

# One script
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test.ps1 -File test/unit/native/str.un
```

The runner shells out to `rust\target\release\unvm.exe run <file>` and aggregates:

- the `=== Result: N passed, M failed ===` line that each script prints through
  `assert.summary()`,
- exit codes and message substrings for the negative manifest,
- the current status of files listed in `test/KnownFailures.txt`,
- the documented divergences listed in `test/KnownDivergent.txt`.

Overall exit code is `0` when every positive script passes, every negative
expectation matches, and no known failure has been fixed silently.

Direct single-file runs work too. Build once with `-Rebuild` (or by hand) and run:

```powershell
.\rust\target\release\unvm.exe run test\unit\native\str.un
```

Rebuild the release binary with

```powershell
rustup run stable-x86_64-pc-windows-gnu cargo build --release --manifest-path rust\Cargo.toml
```

Never run Debug builds; the reader path is not supported.

## Layout

```
test/
|-- assert.un               shared assertion module
|-- test_suite.un           canonical smoke suite (109 assertions)
|-- MANIFEST.txt            group|path list executed by scripts/test.ps1
|-- KnownFailures.txt       scripts that are expected to fail today
|-- KnownDivergent.txt      scripts whose behaviour diverges on purpose
|-- unit/
|   |-- lexer/            literals, escapes, comments
|   |-- parser/           precedence, statement/function/class structure
|   |-- runtime/          operators, scope, control flow, functions, classes,
|   |                     async, errors, date, builtins
|   `-- native/           str, list, dict, set, tup, math, iter, re, random,
|                         inspect, os, sys, fs, io, flow, http, ws
|-- integration/          modules, concurrency, recursion, algorithms
|-- regression/           defects.un (documented behaviour vs actual behaviour)
|-- negative/             crash/parse-error scripts + manifest.txt
`-- fixtures/             import fixtures: circular/, rel_test/, dirmod/, dirall/,
                          circular_main.un
```

Counts at the time of writing:

| Group | Files | Assertions |
|---|---|---|
| suite | 1 | 109 |
| unit + integration | 38 | (1363 - 109) |
| **Positive total** | **39** | **1363** |
| negative (exit-code checks) | 42 checks | 38 matched + 4 documented divergences |
| known (documented defects) | 1 | 6 failing assertions |
| fixtures (not run directly) | 8 | n/a |

## Manifests

`test/MANIFEST.txt` — one `group|path` line per script. Groups: `suite`,
`unit`, `integration`, `regression`, `known`, `negative`. Files under
`test/fixtures/` are import fixtures; the ones whose behaviour is verified by
exit code are registered in the negative manifest, not in `MANIFEST.txt`.

`test/negative/manifest.txt` — one `<path>|<expected exit code>|<expected
substring>` line per script. An empty substring means only the exit code is
checked. Scripts that crash the process (uncaught exception, stack overflow)
must be verified here: they cannot report a result.

`test/KnownFailures.txt` — one path per line. The runner executes these files
with `-Group known`, reports them as `KNOWN FAIL`, and excludes them from the
positive totals. If one of them starts passing, the runner reports
`KNOWN FIXED` and exits non-zero, so the manifest must be cleaned up.

`test/KnownDivergent.txt` — one path per line. Checks that fail on the current
implementation for documented reasons (`order_crash`'s .NET-internal consistency
detection, native stack overflow, the racy `go` scheduling). The runner reports
them as `NEG KNOWN` and excludes them from the negative totals.

## Test file conventions

- `use test.assert as assert` at the top, `assert.summary()` as the last statement.
- Space indentation. Never tabs.
- Assertions state verified behaviour. If documented behaviour and the
  implementation disagree, the assertion states the documented behaviour and the
  file goes into `test/regression/` + `test/KnownFailures.txt` (see below).
- Expected errors: `assert.assert_err(fn() -> <expression>, "substring")`.
- Do not use a global named `e`; `catch e` clobbers it. Use unique catch
  variable names per file (`err_er1`, `err_def2`, ...).
- Do not call `str()` on an error value; use an f-string (`` `{err}` ``).
- Do not call `.close()` on a stream opened with `using`, and do not `map` on a
  pool after `close()`. Both crash the process with uncaught exceptions.
- Do not test unbounded recursion in a positive file; `sys.setcalldepth` above
  the call limit kills the process. Use `test/negative/`.
- Dict literal keys must be expressions: use string keys (`{"min": 1}`), since
  bare `min`/`max` are not defined names.
- Every `fs`/`io` test writes only into a subdirectory created under
  `fs.temp()`, never into the temp root, and never walks the temp root.
- Random values must be checked as ranges, not exact values.
- Comments may be Korean; keep files UTF-8.

## Adding a test

1. Create the file in the right group directory (unit/integration/regression/negative).
2. Write assertions for behaviour you have just verified by running the file.
3. Register the path in `test/MANIFEST.txt` (positive groups) or
   `test/negative/manifest.txt` (exit-code checks).
4. Run `scripts/test.ps1 -File <path>`, then the group, then the whole suite.
5. If the assertion you want to write describes documented behaviour that the
   implementation does not satisfy, do not weaken the assertion. Put it in
   `test/regression/defects.un` (or a new regression file) and register that
   file in `test/KnownFailures.txt`.

## Current status

```
Positive: 31 files, 1150 passed, 0 failed, 0 crashed
Negative: 36 matched, 0 mismatched
Known failures: 1 still failing, 0 unexpectedly passing
Overall exit code: 0
```

The 10 known failures are recorded expectations of documented behaviour, not
workarounds: list slicing with a negative step, `upper_bound` of the last
element, `iter.reverse`, `2 in [1, 2, 3]`, set equality, dict equality,
`iter.zip`, `iter.chain`, a class method shadowing the global `len`, and
`type(none)`. Fixing the implementation is the only path to making them pass.
The full list with the expected values is in
`test/regression/defects.un`, and the history is in `docs/TEST_CLEANUP_LOG.md`.
