# AUDIT_ITEM1.md — Item 1: JIT heterogeneous return kinds (fib24/qsort)

## What the item is

Audit called out that the JIT's plan-time return-kind analysis yields
`INT|INTLIST` (= 0x9, "heterogeneous") for recursive higher-order functions
such as `qsort` and `mergesort`. Items whose proto is heterogeneous are
rejected at every call site (`emit_user_call` / `emit_class_method_call`),
so those protos never compile and the benchmarks run at interpreter speed.

## Investigation summary

### Root cause is `claims_intlist`, not fixpoint order

`analyze_return_kinds` → `analyze_one_proto` iterates a fixpoint over the
register-list state (`reg_float`, `reg_bool`, `reg_list`, `param_regs`, …).
There are two places that can add the `RET_INTLIST` bit to a proto's kind
union; the one that drives 0x9 for our programs is:

```rust
let claims_intlist = code.iter().any(|ins| ins.op == Op::BuildList);
if claims_intlist {
    kinds |= RET_INTLIST;
}
```

Any proto that contains a `BuildList` (`xs = []`) unconditionally claims
INTLIST — regardless of what the function actually returns. qsort does
`smaller = []` and `larger = []`, so it claims INTLIST;  its return arms
produce only INT at runtime and INT at plan time, but the union is
overclaimed to `INT|INTLIST`.

### Fix attempted: make the fixpoint monotone

An impossible-to-stabilise outer fixpoint made the result depend on pass
count. The outer loop was made monotone (`tags.insert(i, cur | tags[i])`),
and the final pass likewise. This is correct (a superset claim is safe —
call sites bail to the interpreter on mismatch) and makes the analysis
order-independent and terminating; it does *not* change the settled
union value, because the union is genuinely 0x9 for qsort.

### Fix attempted: parameter-forward speculation removal

`Return arms returning a parameter` also claimed INTLIST; this was removed
and replaced with a plan-time INT claim (safe because the plan-time
Return-arm runtime probe picks tag 6 vs 0 at run time). This did not
recover qsort either: it is still 0x9 via `claims_intlist`.

### Current status

- `fib24`: **JIT-compiled** (homog INT, 0x1). ~1.3–1.9ms vs ~190ms interpreter.
- `qsort500`: **JIT-rejected** (0x9). Falls back to interpreter (≈ UV_JIT=0),
  benchmarks ~16ms, results correct, `assert.summary()` 5/5 green.
- `mergesort1000`: **JIT-rejected** (0x9). Same as qsort.
- The full test suite / differential remain green:
  - `scripts/test.ps1`: Positive **46 files / 1482 passed / 0 failed / 0 crashed**;
    Negative 41 matched / 0 mismatched; Known 0; exit 0.
  - `scripts/differential.ps1`: **87 files, 0 differences**.

### What a real fix would require

The rejection is *intentional*: with kinds=0x9 and tag=0 or 6 producing
distinct bindings at the call site, the callee's runtime tag must be
dispatched at the call site, and the caller's register typing must then
differ per path. Implementing this is essentially a second, narrower JIT
with dual-suffix emission (one suffix per outgoing tag) — a substantial
project, not a point fix. The sound over-claim is the present behaviour:
overclaim ⇒ refuse at every call site ⇒ interpreter ⇒ correct.

### Alternative: `claims_intlist` scope refinement

`claims_intlist` could additionally require that the `BuildList` register
is actually *reached* by a Return arm (vs. only consumed locally, as is the
case for `smaller = []` and `larger = []`). That would un-claim INTLIST for
the sorts, moving qsort back to homog-INT territory, without changing the
tag-checking logic. Estimated work: moderate (need to extend the register
state with "reg-reaching-Return" flags). **Recommended next pass** if this
item is to be revisited — cheaper than dual-tag JIT and fits the existing
super-set analysis framework.
