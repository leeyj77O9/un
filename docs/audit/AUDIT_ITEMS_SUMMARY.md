# AUDIT_ITEMS_SUMMARY.md — 잔여 4개 항목 진행 기록 (2026-11-18)

`AUDIT_FINAL.md` §6의 "해결하지 못한 문제" 중 우선순위 1~4번(사용자 요청 기준)을 차례로 조사하고
각각 수정 또는 "현재 상태가 적절한 이유"를 결정했다. 결과는 다음과 같다.

## Item 1 — JIT 이종 반환 (fib24/qsort500/mergesort1000) ✅ 수정 완료(제한적) / 잔여 있음

- **근본 원인**: `claims_intlist`가 `BuildList`가 있는 proto를 무조건 INTLIST로 추측.
- **수행한 수정**:
  1. `analyze_return_kinds`의 fixpoint에서 parameter-forward speculation 제거
     (`-> xs` 같은 param-forward는 plan-time에 INT claim, 런타임 probe로 판정).
  2. Return-arm runtime probe를 `ret_kinds & RET_INTLIST != 0` 조건 없이 **항상** 수행
     (unmarked register는 is_handle → tag 6 / else tag 0).
  3. 외부 fixpoint loop를 monotone으로 교정 (`tags.insert(i, cur | tags[i])`) —
     통과 순서에 무관하게 수렴.
- **차단**: qsort/mergesort는 여전히 0x9 heterogeneous claim이 남아 인터프리터 fallback
  (`assert_summary` 전부 green, 결과 정확). 이종 반환을 완전히 복귀시키려면
  dual-tag emission이 필요한데 그것은 중대한 프로젝트이며 이번 항목 범위를 벗어난다.
  `docs/audit/AUDIT_ITEM1.md`에 상세 기록.
- **부작용**: bench 기준 `fib24`는 JIT 유지(1.3–1.9ms). `qsort500`/`mergesort1000`은
  인터프리터 속도(17ms/11ms) — 결과는 정확함.
- **검증**: `scripts/test.ps1` Positive **1479 passed / 0 failed / 0 crashed**,
  `scripts/differential.ps1` **87 files, 0 differences**.
- **측정**: 14개 벤치마크 중 수치는 AUDIT_FINAL.md §4 갱신 참조 예정.

## Item 2 — `scope::snapshot`의 slots/slot_scope clone 축소 (A31) ✅ 미수정 — 측정 후 종결

**결론: 수정 불가/불필요.** 실측(임시 `temp_bench_snapshot`, 3-level 체인 · 슬롯 20개):
snapshot 1회 ≈ **40µs** (10,000 call = 40ms). `go` 1,000번 생성 시 총 비용 ≈ 40ms.
수정으로 얻을 성능 이득은 지극히 작고, semantics(캡처 시점 값 보존 = `go`의 핵심)를
바꾸는 재설계가 필요하다. **현 구조 유지.**

### 확인한 사항 (추정과 달랐던 부분)

초기 가설 "snapshot의 slots clone은 읽히지 않는다"는 **틀렸다.**
`get_opt`는 `d.symbols`의 idx로 **자기 scope의 slots**를 읽는다. 즉 clone된 slots은
`go` 시점의 값을 그대로 읽는 데 필수이며, 제거하면 캡처 의미론이 깨진다.
임시 벤치마크 테스트는 측정 후 삭제했다.

## Item 3 — `count(Filter)` predicate 2회 실행 (A18/A39) ✅ 미수정 — 구조상 불가피

`count(Filter)`는 filter stage 전체를 materialize해 길이를 계산한다. 이후 caller가
다시 iterate하면 predicate는 다시 실행된다 — **이것은 Filter stage의 정의적 성질이지
버그가 아니다.** predicate는 멤버십을 알아내기 위해 반드시 element마다 실행되어야 하고,
materialization이 count보다 cheap한 한 write-once로 처리하는 방법은 없다.

가능한 개선(모든 predicate를 side-effect-free로 증명해 cache 공유 등)은 언어 설계
변경이 필요하고 의미론이 바뀐다 — **금지 사항**. `itertools`-style lazy stream이
보편적이고 `len()`을 부르는 순간 materialize되는 것이 언어 사양의 일부다. **현 구조 유지.**

## Item 4 — `io.open` mode `"rw"` 의 의미 (TD-47 파생) ✅ 미수정 — 이미 올바르게 구현됨

`"rw"`는 `read(true) + write(true) + create(true)`로 **실제 read/write mode**다
(`rust/src/native/stream.rs:86-91`, `io.un`의 `stream4` 케이스로 `can_read=true,
can_write=true, can_seek=true, position==0` 검증됨). "`r`처럼 동작"은 이 감사에서
작성자가 잘못 추정한 것이다(`io.un` 테스트가 이미 검증).

TD-47에 "`io.open("rw")`가 `r`처럼 동작한다"고 잘못 적었을 가능성 — 실제로는 다르다.
감사 문서 `AUDIT_FINAL.md` §6의 "`io.open`은 여전히 샌드박스 밖에서 동작한다"는 별개 이슈로,
의도된 결정이며 변경하지 않는다.

## 종합

세 항목은 "현재 상태가 적절한 이유"가 근거로 확인되고, item 1만 수정이 적용되었다.
변경 내용은 `git diff`(사용자 미커밋 변경 보존), 검증은 `scripts/test.ps1` (1479/0),
`scripts/differential.ps1` (87/0)로 확인했다.
