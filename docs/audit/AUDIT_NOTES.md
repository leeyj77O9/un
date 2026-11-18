# 감사 중간 메모 (subagent 보고 + 직접 검증)

## tests/tooling/docs subagent 보고 (검증됨, 2026-11-18)

### P2
- `scripts/test.ps1` — `Run-Positive`는 `N failed == 0 && exit != 0`인 스크립트를
  `[FAIL]`로 출력하고 `badFiles`를 올리지만, 최종 exit 결정(238-245행)은
  `$pos.failed`/`$pos.crashed`만 본다 → **overall exit가 0으로 유지됨**.
  ARCHITECTURE.md:145의 "exit 1 on any [FAIL]" 불성립. 수정: badFiles 반영.
- **per-test timeout 부재** — `Invoke-Un`은 동기 `& $Exe run`; hang하면 suite 전체가 멈춤.
  AUDIT-S2-9에서 "timeout은 남아 있음"이라고 기록했고 TECHNICAL_DEBT.md는 TD-33을
  closed로 표기 → 문서 모순. (TD-33 실제로는 미구현.)
- `differential.ps1`이 "JIT on" lane에 `UV_JIT=1`을 명시 설정 — 기본값이 바뀌면 lane이
  조용히 어긋남 (P3).
- `unbc-roundtrip.ps1`이 source tree에 `<file>.unbc` 쓰고 finally에서 삭제 (P3, gitignore됨).

### P3 (docs drift) — .un count 88 (not 86/89), rust/src 83 files/36,518 lines
- AGENTS.md:11 "81개 파일 / 33,633줄 … 86개" → 83 / 36,518 / 88.
- README.md:281, docs/README.md:283 "1415 passed" → 실제 1419.
- docs/TEST_GUIDE.md:78-79,143 "1415 / (1415-109)" → 1419 / 1310.
- docs/ARCHITECTURE.md:152 "204 Rust unit tests" → 205. (TEST_GUIDE.md:149는 205로 정확)
- docs/PROJECT_AUDIT.md:17 "89 .un / 202 tests", :25, §6 "1391/200" — stale,
  "현재 상태"로 프레이밍되어 있어 misleading.
- docs/REFACTOR_LOG.md:580 "81 files / 33,633 / 86" stale.
- D-072→D-077: PROJECT_AUDIT.md:17, REFACTOR_SUMMARY.md:107, AGENTS.md:114.
- DECISIONS.md D-012: status "Proposed"인데 참조하는 test/negative/go_loop_var.un은
  삭제됨; D-054는 같은 기능을 Accepted/구현으로 기술 → 모순 (P3).
- D-065: "cargo test 199/199" stale (205).
- vendor/README.md:14 region upstream 표기 오류 (Amanieu/region이 정확) — P3.
- TD-31 인용(vendor/README.md:17-18이 LICENSE 존재를 claim)은 현재 README와 불일치 — P3.
- BENCHMARK.md:196 "math10000 unjit +139312.8%", :208 sin20000 +4753.2% — baseline CSV가
  물리적으로 불가능 (0.0156ms/10k calls) → bogus regression 항목으로 표시해야 함 — P3.
- bench.ps1: 결과 파일 존재/파싱 검증 없음, geomean이 누락 lane을 조용히 제외,
  exit code가 benchmark 실패만 반영 — P3.
- Negative suite weak spots: `empty.un|0|`, `comment_only.un|0|` (empty substring),
  단일문자 substring 행 여럿 (`space_str.un|0|1` 등) — anchored 권장 — P3.
- Missing coverage: JIT corpus (test/**.un에 JIT 언급 없음), `.unbc` corruption negative
  tests, go-future body throw → `wait` 재-raise 테스트 부재, ws/http 얇은 lane.

### 직접 검증
- test.ps1 218-223행 확인: `$pos`가 all/unit 등에서만 설정됨; negative/known 그룹에선
  `$pos` 미사용 → 수정 시 null guard 필요.
- DECISIONS.md D-001..D-077 전부 unique (subagent 확인).

## clippy 직접 측정
- `cargo clippy --release`: **1 error** — `jit_bc.rs:3499` self-assignment
  (`reg_float[ins.a] = reg_float[ins.a];` in `Op::Unwrap` arm; `_ => {}`가 커버하므로
  arm 삭제로 해결 가능, 의미 불변). **사용자 미커밋 파일** — 보고 후 처리.
- 155 warnings (대부분 style: borrowed expression implements required traits 33,
  cast same type 13, manual 11, …). rustc warning은 0.
- clippy::type_complexity (PLAN_CACHE), redundant guard (Op::MakeClass) 등 경미한 것 포함.

## 벤치 기준선 (2f67935, `-Group un`)
unjit: int_ops 2.968, fib24 1.396, nested_sum_200 0.013, qsort500 1.040,
mergesort1000 1.773, rle1994 14.707, dict5000 5.798, iter10000 5.021,
math10000 35.445, class1000 1.312, sieve30000 5.319, matrix48 12.514,
sin20000 57.996, call200000 210.584, regex6000 1.282 (ms).
(vm 컬럼은 "n/a"로 출력됨 — 사용자 수정 bench.ps1의 현재 동작.)
