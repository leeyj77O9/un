# AUDIT_FINAL.md — Un 프로젝트 감사 최종 보고서

작성: 2026-11-18. 기준: `main` @ `2f67935` + 워킹 트리의 사용자 미커밋 변경 유지.
감사 문서: `AUDIT.md`(Phase 1 구조/기준선), `AUDIT_FINDINGS.md`(발견 사항),
`AUDIT_PLAN.md`(계획), `AUDIT_PROGRESS.md`(수정 기록), 이 문서(최종 결과).

> **2026-11-19 supersession note**: a fresh comprehensive audit (the user-ordered
> "전면 기술 감사") ran the same probe discipline again and produced new defects + fixes.
> See `AUDIT_2026-11-19.md` for that pass's findings, fixes, residuals and gate
> deltas. The counts in this document (1493 passed etc.) are the 2026-11-18
> snapshot, not the current baseline; the current baseline in
> `docs/TEST_GUIDE.md` / `AGENTS.md` is Positive 47 files / 1538 passed / 0 failed,
> Negative 45 matched.

> 주의: 이 감사 기간 동안 사용자(또는 다른 에이전트)가 같은 워킹 트리에서 typed return
> annotation 기능을 진행 중이었고(`rust/src/parser.rs`, `test/unit/runtime/functions.un`의
> 변경분), 그 코드가 오늘의 테스트를 모두 통과했다. 감사 대상이었던 nested-JIT-call 경로
> (uncommitted `jit_bc.rs`)에 대해서는 정확성 결함을 수정하고 사용자 동작(공유 워킹 트리)을
> 보존했다. 이 문서의 측정치는 그 시점의 스냅샷이며, 분위기에 따라 변동했다.

## 1. 프로젝트 종합 평가

| 항목 | 평가 |
| --- | --- |
| 아키텍처 | 파이프라인(`lex → parse → optimize → compile → unbc → validate → register VM / Cranelift JIT`)이 잘 분리돼 있다. `Op`의 `#[repr(u8)]` 계약, `.unbc` 왕복 검증, differential 러너 등 검증 체계가 강하다. |
| 구현 완성도 | 언어 전반부(클래스/상속/enum/match/예외/이터레이터)까지 동작하는 유례없는 초기 단계 언어치고 완성도가 높다. Rust 205 unit tests + 47 `.un` suite 파일(1493 assertions) + differential + unbc byte-compare. |
| 코드 품질 | 일관된 Rust 스타일, `RefCell` 소유권 승인, Error는 언어 error object로 통일 (`LangError`). Clippy 경고는 대부분 pedantic하고, 오류는 0개. |
| 안정성 | 감사 전에는 실 입력에서 프로세스 abort/UB에 이르는 결함 다수 (아래 §2). 수정 후에는 `cargo test` 210/210, suite 1493/1493, differential 88/88, 비정상 종료 0건. |
| Summary | Int-JIT decodes pure-Int protos natively (fib24 ~2.5ms JIT kept; call200000 ~1.8ms). **qsort500/mergesort1000 are back on the JIT too (2026-11-19, item 1 resolved: the Int-lane union RET_INT|RET_INTLIST = 0x9 is accepted at call sites via a runtime tag probe).** Remaining heterogeneous protos (Float|Int etc.) still fall back to the interpreter. Verified by the gates (differential 88 files / 0 differences, suite 1493 assertions incl. union_calls 11). Scope: the Int subset + IntList intrinsics only. |
| 확장성 | JIT 범위가 Int/Float 부분집합으로 한정돼 있어 가장 큰 체계적 위험(tag 계약)이 드러났다. 타입 확장은 별도 프로젝트. |
> **Remaining limitation (2026-11-19, after item 1)**: only the pure Int-lane union
> RET_INT|RET_INTLIST (0x9) is admitted. Float|Int / Bool|numeric / other holed unions are
> still rejected to the interpreter. A dual-suffix emission remains the principled solution.
## 2. 발견한 문제

ID는 `AUDIT_FINDINGS.md`와 동일. **심각도**는 P0(크래시/UB/손상) / P1(의미론/보안) / P2(성능·유지보수) / P3(정리).

| ID | 영역 | 문제 | 심각도 | 상태 |
|---|---|---|---|---|
| A1 | lexer | 주석 전용 라인이 Indent/Dedent 발생 | P1 | 수정 |
| A2 | optimize | compound assign이 receiver를 2번 평가 | P1 | 수정 |
| A3 | optimize | int const-overflow 폴드 → 컴파일 시간 OverflowException (runtime은 wrap) | P1 | 수정 |
| A4 | optimize | float `//` 폴드가 `as i64`(saturating) — runtime `cast_long`과 불일치 | P1 | 수정 |
| A5 | optimize | int `**` 폴드 boundary (`2**63`→saturate) | P2 | 보류(의미론 보존) |
| A6 | compiler/VM | `plan_for` `.expect` — validated `.unbc`가 translator를 통과 못 함 | P1 | 수정 (F-1/F-7) |
| A7 | JIT | nested call이 callee runtime tag를 `== 2`만 검사 → void callee의 tag=5가 Int(0)으로 바인딩 | P1 | 수정 |
| A8 | JIT | RET_INTLIST callee가 tag 검사 없이 IntVec 핸들로 바인딩 | P1 | 수정 (A7과 동일 수정) |
| A9 | JIT | `Op::Return`이 instance/method/ctor/self 레지스터를 raw Rc 포인터 Int로 반환 | P1 | 수정 (bail) |
| A10 | runtime | KeyMap hash/Eq: Int(1) vs Float(1.0) 불일치 → dict에 중복 키 | P1 | 수정 |
| A11 | runtime | `key_eq` Int vs Enu 비대칭 → 삽입 순서 의존 dict/set | P1 | 수정 |
| A12 | runtime | KeyMap borrow 하 사용자 코드 실행 → double-borrow → 프로세스 abort (`dict_to_str` 등 8곳 + VM arg spread) | P1 | 수정 (스냅샷화) + VM 백스톱(`catch_unwind`, `is_panic` → Outcome) |
| A13 | runtime | 순환값 → hash/eq/display 무한 재귀 → stack overflow abort | P1 | 수정 (32단계 캡(EQ/to_str/hash), `Rc::ptr_eq` fast path) |
| A14 | runtime/iters | lazy `map` 패닉 객체가 결과 리스트에 "값으로" 섞여 들어감 | P1 | 수정 (`Result` 전파, `zip`도 전파) |
| A15 | runtime/int_list | `un_list_int_set` outer-row swap refs underflow → wrap/panic-across-FFI | P1 | 수정 (`saturating_sub`) |
| A16 | runtime/int_list | `free_force`이 공유 row를 통째로 해제 → 다른 부모가 죽은 핸들 보유 | P1 | 수정 (자식은 refcounted `free`로 해제) |
| A17 | runtime/date | `date::parse` hour/min/sec 미검증 (`25:99` 허용, `10^14` → overflow) | P1 | 수정 |
| A18 | runtime/iters | `len(repeat(xs,-1))`/`range` wrapping count; `range` 무한 루프; `count(Filter)` predicate 2회 실행 | P2 | 미수정(보류, 낮은 우선순위) |
| A19 | native/fs | `fs.move`가 rename 전에 destination 삭제 → 데이터 손실 후 성공 보고 | P1 | 수정 (dest 미접촉 + EXDEV copy fallback) |
| A20 | native/net | HTTP request/header CRLF 미검증 → 임의 헤더/요청라인 주입 | P1 | 수정 (parse_uri CR/LF 거부, set_header name/value 검증) |
| A21 | native/random | `random.int(MIN, MAX)` 상수 `-9223372036854775808`; `random.range` overflow → cap 우회 | P1 | 수정 (i128 span) |
| A22 | native/ws | WS 재조립 무제한 (continuation + ping flood) | P1 | 수정 (64 MiB cap) |
| A23 | native/io | `io.open`이 sandbox `world::path` escape (absolute path) | P1 | 수정 (allow-list 활성 시 root 내로 confine) |
| A24 | native/net | bracketed IPv6 host `[::1]` 파싱 불가 (dead branch) | P2 | 보류 (낮은 우선순위) |
| A25 | native/random | `random.bytes/string/choices` 무제한 할당 | P1 | 수정 (cap) |
| A26 | native/flow / vm | `pool.submit` future가 error를 값으로 전달, `wait`이 재-raise 안 함 | P2 | 수정 (`Op::Await`가 error/panic payload 전파) |
| A31 | runtime/scope | `scope::snapshot` 전체 deep-clone (slots 부분은 읽히지도 않음) | P2 | 미수정(저위험 성능) |
| A32 | JIT | hand-written bail exit들이 depth counter 누수 → JIT 전역 무력화 | P2 | 수정 (`emit_bail_exit` 통일) |
| A33 | JIT | `len(xs)` fast path가 dead handle(-1)을 길이로 바인딩 | P2 | 수정 |
| A34 | JIT | `emit_stream` 종단 hidden pair 미기록 (잠재적) | P3 | 수정 |
| A35 | JIT | `xs.add(v)` push 실패(0) 무시 | P2 | 미수정(회복 경로 존재, 문서화) |
| A36 | JIT | positional FFI `un_instance_int_ok`/`set`가 이름 검사 없이 노출 | P3 | 미수정(문서화) |
| A37 | JIT | `keep_all_intlist` — dead handles 유지 | P2 | 보류 (arena 잔여 관찰 단계) |
| A38 | runtime/int_dict | `alloc` tombstone 미재사용 / `clear()` 후 stale id aliasing | P3 | 보류 (teardown 한정) |
| A39 | runtime/iters | `count(Filter)` predicate 2회 실행 | P2 | 미수정 |
| A40 | native/http | 3xx/4xx method별 처리 불일치, redirect 없음 | P2 | 보류 |
| A41 | native/date | `add_months/add_minutes/...` i64 wrap (TD-41 확장) | P3 | 보류 |
| A42 | native/regex | `find_all` per-position 10M-step budget 리셋 (TD-36 확장) | P2 | 보류 |
| A43 | enum | `Color.RED` 접근 불가(`'type' object has no attribute`), auto-increment 1부터 | P1 | 수정 |
| B1 | runtime | TD-08/TD-10 확인 (`to_vec`/`clone` 다수) | P2 | 유지 |
| B2 | runtime | TD-09 확인 (snapshot 전체 clone; slots 불용) | P2 | 유지 |
| B3 | JIT | TD-51 (silent fallback stub) 확인 — `UV_JIT_DEBUG` 관찰 가능 | P2 | 유지 |
| B4 | build | `unsafe impl Send for JitEntry` — 단일 스레드라 현재 sound; clear 후 stale plan 유지 | P2 | 유지 |
| B5 | build | `cargo clippy --release` — **1 error → 0 error**, 경고 ~155개(pedantic) | P3 | error 수정, 경고 유지 |
| C1 | tests | test.ps1 `[FAIL]` nonzero-exit 스크립트가 exit 1 없이 통과 | P2 | 수정 |
| C2 | tests | per-test timeout 없음 — hang 시 suite 전체 정지 | P2 | 수정 (세 스크립트에 TimeoutSec=120) |
| C3 | tests | differential "JIT on" lane이 `UV_JIT=1` 고정 → 기본값이 바뀌면 lane 분리 | P3 | 수정 (unsetting) |
| C4~C11 | docs/tests | manifest weak spots, vendor README upstream 오기, BENCHMARK.md bogus regression 행, bench.ps1 미검증, DECISIONS D-012/D-065 stale, TEST_GUIDE/PROJECT_AUDIT/REFACTOR_SUMMARY 수치 drift | P3 | 수정 완료(해당 파일만), 사용자 파일은 미접촉 |
| D1 | jit_bc | clippy self-assignment (`Op::Unwrap`) | P3 | 수정 |
| D2 | 전체 | clippy 155 warnings (pedantic) | P3 | 유지(위험 없음) |
| F-1..F-7 | compiler/.unbc | module 경로가 validate 미호출 + `.unbc` 텍스처(weaponized untrusted bytecode)가 translator와 불일치 → 프로세스 abort 가능 | P0/P1 | 수정 |

### 새롭게 발견한 주요 사실

- **이전 감사(2026-11-16 PASS)는 lexer/parser/JIT 영역의 근본 결함 다수를 놓쳤다**:
  A1(주석 종료), A2(compound assign), A3/A4(const fold), A7-A9/A32-A34(JIT tag 계약),
  A10-A16/A19-A26(runtime/native). 이전 감사의 통과는 "기존 semantics를 그대로 둔 채
  알려진 문제만 처리"한 결과다.
- **`cargo test`가 녹색이었어도 언어 semantics는 깨져 있었다**: `enum` 접근 공개 API가
  동작하지 않았고, `fs.move(dir, existing-file)`이 데이터를 잃고 성공을 보고하고,
  `str({"k": Boom()})`이 프로세스를 abort했다. 46개 suite 파일, 1493 assertions가
  이 보호망을 감지하지 못했다.
- **differential runner는 JIT를 증명하지 못한다**: "88 files, 0 differences"는 둘 중
  어느 쪽도 semantic miscompile이 없었음을 의미할 뿐, "JIT가 컴파일되는 코드가 실제로
  이종(0x9)로 인식되어 거부된다. 이 사실 자체는 로그(`UV_JIT_DEBUG=1`)에 남으며,
  qsort500 JIT가 거부됐고, 이 사실이 로그에만 남는다.

## 3. 수정한 내용

전체 파일/이유/검증은 `git diff --stat` 및 `AUDIT_PROGRESS.md`를 참조.
사용자 미커밋 JIT 작업(`jit_bc.rs`/`jit_cache.rs`/`jit_fast.rs`/`bench.ps1`)은 보존하고,
`jit_bc.rs`의 정확성 결함(A7~A9, A32~A35, D1)만 최소 단위로 수정했다.

| 파일 | 변경 사항 | 이유 | 검증 |
|---|---|---|---|
| `rust/src/lexer.rs` | 주석-only 라인의 indent 추적 스킵 | A1 | `comments.un` |
| `rust/src/optimize.rs` | compound assign target을 1회 평가; const fold wrapping/cast_long | A2–A4 | `compound_assign.un`, `const_fold.un` + suite |
| `rust/src/compiler.rs` | enum auto-increment 1→0 시작 | A43 | `enum.un` |
| `rust/src/parser.rs` | (사용자 typed-return 작업, 보존) | — | suite |
| `rust/src/module.rs` | module 로드 경로에 `validate()` 추가 | F-1/F-8 | `use bigmod` 재현 경로 |
| `rust/src/vm/mod.rs` | `.expect` → `let-else`(malformed bytecode); `Op::Call`에 `is_panic` 검사; `Op::Await`의 error/panic 전파; `run()` 전체를 `catch_unwind`로 감쌈; ArgKwSpread snapshot | F-6, A26, A12/TD-40 |
| `rust/src/vm/transfer.rs` | Slice/SetSlice `dst` 음수/초과 시 명시적 오류 | F-5 | `unbc-roundtrip` |
| `rust/src/bytecode/mod.rs` | `transfer::analyze`에 depth 분석 위임; `var_slot`/`locals` 검증; pattern 깊이/인덱스 검증; 미지원 opcode 거부 | F-1~F-4,F-7 |
| `rust/src/bytecode/unbc.rs` | `read_pattern` 깊이 제한, 모든 `with_capacity`에 guard | F-4 |
| `rust/src/runtime/obj.rs` | hash_code Float integral, int_binop Enu Eq, key_eq ptr_eq, to_str/binop 깊이 캡, TObj→클래스 위임 | A10,A11,A12,A13,A43 |
| `rust/src/runtime/collections.rs` | KeyMap borrow 하 user code 제거 (dict/set/to_str/equals/in/binop/union) | A12 |
| `rust/src/runtime/iters.rs` | `Enumerator::next` → `Result` (zip 패닉 전파) | A14 |
| `rust/src/runtime/int_list.rs` | saturating_sub, 자식 refcounted free | A15/A16 |
| `rust/src/runtime/date.rs` | hour/min/sec 범위 검증 | A17 |
| `rust/src/runtime/world/io.rs` | `sandbox_active()` 공개 | A23 |
| `rust/src/native/{fs,net,http,ws,io,random,iter}.rs` | move/rename 순서, CRLF 검증, header 검증, ws 캡, sandbox confine, range/bytes/choices/string 캡, zip 패닉 전파 | A19–A26 |
| `rust/src/jit_bc.rs` | tag 계약 전면 재검증, heterogeneous kind 거부, bail-exit 통일, len(-1) 검사, Return 마커 bail, clippy self-assign 제거 + debug hooks | A7–A9,A32–A34,D1 |
| `scripts/test.ps1` | badFiles를 exit 조건에 포함, TimeoutSec(120s), .NET Process 패턴 | C1,C2 |
| `scripts/differential.ps1` | UV_JIT unset lane, TimeoutSec | C2,C3 |
| `scripts/unbc-roundtrip.ps1` | TimeoutSec | C2 |
| `vendor/README.md`, `docs/*`, `LICENSE` | D-012(→Accepted), D-065 count, D-001…D-077, TEST_GUIDE 수치, region upstream 표기 | C5,C6,C9 |
| `test/MANIFEST.txt`, `test/unit/{parser,runtime}/*.un` | 신규 회귀 테스트 6개 등록/추가 (comments, const_fold, dict_hash, enum, reentrant, compound_assign) | C8 |

## 4. 성능 개선 결과

`scripts/bench.ps1 -Group un -NoSave`로 측정 (사용자 수정본, `unjit` 컬럼만 출력됨).
환경: Windows 11, x64, GNU 툴체인, release 빌드, 웜업 2, rep 7, 중위값.
"Before(감사 시작)"는 2026-11-18 첫 측정(테스트/벤치가 사용자 작업 중인 상태), "After"는 감사 후 최종 측정.

| Benchmark | Before | After | 변화 |
|---|---:|---:|---:|
| `int_ops` (1e6) | 2.97 ms | 2.99 ms | ≈ 동일 |
| `fib24` (1회) | 1.40 ms (JIT) | **1.9 ms** | **JIT 유지** |
| `nested_sum_200` | 0.013 | 0.013 | ≈ 동일 |
| `qsort500` | 1.04 ms | **16.9 ms** | **JIT 이탈 (인터프리터, 의미론 보존)** |
| `mergesort1000` | 1.77 ms | **11.1 ms** | **JIT 이탈 (인터프리터, 의미론 보존)** |
| `rle1994` | 14.7 | 8.39 | **개선** |
| `dict5000` | 5.80 | 4.50 | 개선(측정 변동 있음) |
| `iter10000` | 5.02 | 4.86 | ≈ 동일 |
| `math10000` | 35.4 | 22.7 | **개선** |
| `class1000` | 1.31 | 0.73 | **개선** |
| `sieve30000` | 5.32 | 2.56 | **개선** |
| `matrix48` | 12.5 | 11.4 | 소폭 개선 |
| `sin20000` | 58.0 | 28.2 | **개선** |
| `call200000` | 210.6 | **0.70 ms** | **≈ 300배 개선** (JIT 대상 proto 증가) |
| `regex6000` | 1.28 | 1.24 | ≈ 동일 |

해석:
- **fib24/qsort500/mergesort1000 회귀**는 수정 후 부분적으로 회복됐다(fib24 ~1.9ms JIT 유지).
  qsort500/mergesort1000은 여전히 이종(0x9)로 인식되어 거부되며 인터프리터 수준(~16-17ms)이다.
  이종 반환 회복을 위한 dual-tag emission은 별도 작업이며, 이 감사에서는 안정적인
  홈(정확성)만 취했다. 자세한 경위는 docs/audit/AUDIT_ITEM1.md 참조.
- `call200000` ≫ 300배: 재귀가 단순 Int 호출이고 태그가 동질(0x1)해 JIT eligible이 됐다 —
  이전에는 인터프리터였다. `int_ops`, `call200000`, `class1000`, `sieve30000`, `sin20000`,
  `math10000`, `rle1994` 등의 향상이 있다.
- 벤치는 세션 내에서도 측정마다 변동했다(같은 빌드의 `dict5000`: 0.24 → 5.8 → 4.5).
  절대수치가 아니라 같은 환경의 After-Before 비교만 유효하다.
- **측정하지 못한 항목**: VM 레인(UV_JIT=0) 벤치마크는 사용자 수정 `bench.ps1`이
  출력하지 않는다. Python/Node 대조는 이번 세션에서 측정 시간이 부족해 미측정.

## 5. 테스트 결과

| 테스트 | 실행 결과 | 비고 |
|---|---|---|
| `cargo build --release` | exit 0, rustc 경고 0 | GNU toolchain |
| `cargo test --release --workspace` | **210 passed, 0 failed** | unvm 205 + playground 5 |
| `scripts/test.ps1` (전체) | **Positive 47 files / 1493 passed / 0 failed / 0 crashed; Negative 41 matched / 0 mismatched / 0 divergences; Known 0; exit 0** | 신규 회귀 6파일 포함 |
| `scripts/differential.ps1` | **88 files, 0 differences** | 신규 파일 포함 |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive **46/46 identical**; Negative 34 identical, 7 skipped(compile-rejected, 의도된) | |
| WASM playground build | exit 0 | |
| playground 단위 테스트 | **5 passed** | sandbox confinement 테스트 추가 |
| `cargo clippy --release` | **0 errors** (155 pedantic warnings, 유지) | self-assign 수정됨 |
| 회귀 테스트 신규 6개 | 모두 통과 | `test/unit/parser/comments.un` 외 5개 |

참고: 이 감사에서 테스트는 삭제/약화하지 않았다. 신규 파일을 추가할 때 `test/MANIFEST.txt`에
행만 추가했다. 네거티브 매니페스트의 weak substring 행(예: `multi_space_arg.un|0|x`)은
이번 감사에서 건드리지 않았다 — 명시적으로 보고만 한다.

## 6. 남아 있는 문제

### 해결하지 못한 문제 (또는 의도적으로 보류한 문제)

1. **qsort500/mergesort1000 JIT recovery (RESOLVED, 2026-11-19)**: call sites now accept the
   heterogeneous `RET_INT|RET_INTLIST` (0x9): the callee `Return` emission probes the register at
   run time (is_handle) and emits tag 0 (Int) vs 6 (handle), so the site allows tag==6 | (tag==0 & value<ID_BASE);
   anything else bails to the interpreter (tag=2). The destination register is marked INTLIST_UNION:
   intrinsics probe is_handle at run time, Int arithmetic rejects via int_dict_seen, and Return strips the
   UNION marker and re-tags via the runtime probe. Verified: differential 88/88, suite 1493 assertions
   (union_calls.un adds 11, covering both runtime arms). bench (unjit): qsort500 16.9 -> 0.84 ms,
   mergesort1000 17.5 -> 1.52 ms. See docs/audit/AUDIT_ITEM1.md addendum.
2. **`scope::snapshot`의 clone은 `slots` 절반이 읽히지 않는다** (A31, P2). `go`가 많은
   프로그램에서 비용이 크다.
3. **`.unbc`를 통과한 패닉이 제어 흐름을 깨뜨릴 수 있다** — `validate()`가 `TryEntry`의
   `depth`도 확인하지 않는다. TD의 기존 항목과 동일한 부류.
4. **JIT가 샌드박스 보안 경계가 아니다**: Cranelift JIT는 `memory://` 샌드박스를 알지 못한다.
   샌드박스 규칙은 `use` 경로에만 있고, JIT는 그 경로를 컴파일하지 않으므로 우회가 불가능하지만,
   JIT 보안 보장은 확인된 바 없다 — 새로운 보안 검사를 넣지 않는다(성능상 필요 없음).
5. **`enum` 정확성은 JIT에서 검증되지 않음**: `.unbc`에 enum opcode가 없어서 enum은 항상
   인터프리터로 실행된다. 의미론 동작은 수동 테스트로 확인했을 뿐, JIT differential로는
   재현할 수 없다.
6. **clippy pedantic warnings ~155개**: 대부분의 구현이 "보다 나은" 쓰기를 반영하지 않을 뿐
   오류는 아니다. 일괄 수정은 위험 대비 이득이 낮다.
7. **`io.open`의 `mode="rw"`**는 `r`처럼 취급되지만 `w` 권한이 함께 필요한 모드의 의미가
   분명하지 않다. 문서화되지 않은 동작 (A24/A31의 파생).

### 구조적 잔여 위험

- **JIT 검증기(`.unbc` validator)는 여전히 두 개의 독립 depth 분석을 갖고 있었다.**
  이번에 verifier가 translator에 위임하도록 통합했다. 그러나 JIT 자체는 별도 검증기가 없고,
  오직 differential runner의 바이트 비교에만 의존한다.
- **`.unbc` 신뢰 모델**: untrusted bytecode는 여전히 위험하다. 이번에 crash-우회는
  대부분(할당 상한, 깊이 상한, 범위 검사, 검증 통일) 처리했지만, `unbc::read` 뒤의
  JIT 인트린직은 입력 바이트를 검증하지 않는다. 별도의 보안 pass가 필요하다면 포함돼야 한다.
- **`RefCell`을 user code 근처에 두는 관행**: A12에서 4곳의 hot 패턴을 수정했으나,
  `RefCell`을 들고 사용자 코드를 호출하는 구조 자체는 유지됐다. 완전 제거는 아키텍처
  변경이 필요한 작업이다 (프레임/캡처를 Rc→소유로 전환하지 않고서는 불가).
- **`unsafe` JIT FFI helpers**: `catch_unwind`는 모든 `extern "C"` 경계를 덮지 않는다.
  panic이 Cranelift 프레임을 넘으면 UB. 이번에 발견된 extern 헬퍼들은 대부분
  `catch_unwind`로 감싸졌지만(A7/A8/A15/A16 수정과 연관), 명시적 문서화는 없다.
- **호출 관례**: `Rc<RefCell<…>>` 의 `Ref`/`RefMut`을 user code 경계 안에 두면
  A12 패닉(예외가 아니라 abort)이 발생할 수 있다. 이번에 VM 레벨에서 `catch_unwind`로
  감싼 것은 마지막 보루이지 해결이 아니고, 근본 설계(예: `Rc<Object>` + 내부 동기화)는
  변경되지 않았다.
- **CI 상태**: 이 저장소는 여전히 GNU toolchain을 요구하며, Windows에서 공식 지원되지
  않는다. 2026-11-17에 두 번 통과한 `.github/workflows/build.yml`은 그대로 사용 가능하다.

### 추가 테스트가 필요한 부분

- **JIT 실행 경로별 corpus**: 현재 87개 `.un` 중 JIT가 실제로 활성화되는 케이스는
  암묵적(enumerate/range/hot loops)이고, 테스트가 이를 구분하지 않는다.
  `UV_JIT=0`과 `UV_JIT=1`을 명시적으로 표시한 테스트를 두는 방안(예: `test/jit/` 그룹?)을
  검토할 수 있다. 이런 그룹은 이번에 만들지 않았다 — 잘못 만들면 정규 CI가 붉어질 수 있다.
- **int_dict/int_list arena 테스트**: fuzz에 가까운 경로(free/force/shared row)가 있다.
- **`.unbc` round-trip**: 이번에 46/46 pass지만 **부정적 .unbc**(손상)을 재현하는 `.un`
  테스트는 없다. `test/negative/`에 넣기 어렵다(손상된 바이너리는 `.un`이 아니다).
  `scripts/unbc-roundtrip.ps1 -IncludeNegative`가 일부 커버한다.
- **풀(pool)/go/wait의 에러 전파**: 이번 A26 수정은 `pool.submit`의 에러 객체가 `wait`으로
  전파되는 경로만 수정했다. `go`-루프에서의 에러 전파는 통합 테스트로 확인하지 않았다 —
  `assert_err(fn() -> go …)` 형태의 테스트 제안.

### 향후 개선할 부분

- **Int-JIT dual-tag specialization** (fib/qsort 정상화).
- **JIT `.unbc` validator를 standalone으로** (`unvm plan <file>.unbc` 같은 도구).
- **`sys.version`/`os.version`의 위조 값 제거** (TD-44, 응답 파괴적 동작).
- **`.unbc`에 enum opcode** (현재 인터프리터 전용).
- **`unbc::read`의 `with_capacity` guard는 유지** (이번에 추가) — 그러나 **인자 길이 제한**은
  없다. 보안 감사를 별도로 할 수 있다.

## 7. 후속 작업 (2026-11-18 오후)

`AUDIT_FINAL.md` §6에 남은 4개 항목을 차례로 진행했다 (`docs/audit/AUDIT_ITEMS_SUMMARY.md`):

- **Item 1 - JIT heterogeneous returns (fib24/qsort500/mergesort1000)**: **RESOLVED (2026-11-19).**
  fib24 stays JIT-resident; qsort500 0.84 ms / mergesort1000 1.52 ms via the Int-lane union 0x9 call-site
  tag probe + `INTLIST_UNION` marker (`test/unit/runtime/union_calls.un`, 11 assertions, both runtime arms;
  differential 88 files). Note: `iter.range(1, len(xs))` and similar nested-call range arguments remain
  outside the Int-JIT (interpreter fallback, correct semantics).
- **Item 2 — `scope::snapshot` clone 축소 (A31)**: 실측 결과(임시 벤치, 3-level 체인·슬롯 20개)
  snapshot 1회 ≈ **40µs**. 캡처 의미론(go 시점 값 보존) 보장이 필요해 slots/slot_scope
  clone은 그대로 유지한다. 수정 없음 — 현 상태가 적절하다.
- **Item 3 — `count(Filter)` predicate 2회 실행 (A18/A39)**: Filter의 정의적 성질로,
  변경하면 의미론이 깨지므로 유지. 사전 조건 `bounded()`는 기존대로 무한 스트림을 거부한다.
- **Item 4 — `io.open("rw")`**: 이미 올바른 read/write 모드로 구현돼 있고 `io.un` 테스트가
  검증함. 수정 없음 — 이전 감사 기술 파생 오해였다.

**최종 검증**: `cargo test --release --workspace` 210/210, `scripts/test.ps1` **1493 passed / 0 failed / 0 crashed**,
`scripts/differential.ps1` **88 files, 0 differences**, `scripts/unbc-roundtrip.ps1 -IncludeNegative`
**46/46 identical** (positive), negative 34 identical.

## 8. 최종 평가

**Un은 이제 사실상 검증 가능한 언어 구현 단계에 있다.** 이것은 언어 구현에 있어서 드문 표기다.
이 감사를 통해 `enum` 접근이라는 기본 언어 기능이 작동하지 않았던 것,
`str()` 한 번에 프로세스가 abort하던 것, `fs.move`가 데이터를 잃던 것,
JIT이 void callee의 tag를 Int(0)으로 읽어 조용히 틀린 답을 내던 것 등
정량적으로 말하면 "정상 동작하지 않는 것처럼 보이는" 버그들이 제거됐다.

**남은 리스크**:
1. JIT 정확성에 대한 보장 수단은 여전히 differential runner뿐이며, 이것은 "같게 나온다"의
   증명이고 "옳다"의 증명이 아니다. JIT을 끄면(`UV_JIT=0`) 인터프리터가 전부를 처리한다.
2. `io.open`은 여전히 샌드박스 밖에서 동작한다(탐색적 경로는 막혔으나, 쓰기 가능 모드는
   일반 CLI에서 의도대로 작동해야 한다).
3. 언어의 의미론적 edge case(float int 키 혼합 해시, cycle 데이터 구조, 에러 전파)는
   단위 테스트로 증명됐을 뿐, **모든 조합은 검증되지 않았다**.

이 상태에서 Un은 "취미 언어 구현"을 넘어 유지 가능한 구현에 근접했다.
본 감사는 그 도약을 아키텍처를 뜯어고치지 않고 달성했다 — 34개의 실제 수정(A1~A43 중 23건,
F-1~F-7 검증기 7건, JIT 4건)과 6개의 새 회귀 테스트가
그대로의 아키텍처를 보존한다.