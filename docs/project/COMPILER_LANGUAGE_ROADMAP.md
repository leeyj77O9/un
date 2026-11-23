# COMPILER_LANGUAGE_ROADMAP.md — 컴파일 언어로서의 공백 해소 계획

> 작성: 2026-11-23, ARC-00. 근거: `docs/project/COMPILER_LANGUAGE_GAPS.md` 분석(에이전트 보고 취합).
> 이 문서는 계획이며, 구현은 Work 단위로 분배한다. 상태는 `.opencode/SESSION_STATUS.md`에 기록한다.

## 1. 우선순위와 파도(Wave)

| Work | 과제 | 담당 | 선행 | 허용 파일 | 완료 조건 |
|---|---|---|---|---|---|
| W-2026-11-24-LANG3 | 모듈 최상위 의미 확정(silent 무시 제거) | VM-02 + ARC-00(main.rs 통합) | 없음 | `module.rs`, `compiler.rs`, `vm/**`, `runner.rs` | top-level 부수효과가 `un run`과 동일하게 실행되거나, 명시적 컴파일 오류. 조용히 무시하는 경로 0 |
| W-2026-11-24-LANG4 | 컬렉션/Float closure 커버리지 | NRT-06 + CMP-05 + NAT-07 | 없음 | `nrt/**`, `typeck.rs`, `tir.rs`, `natcode.rs`, `unrt.rs`, `native/**` | 중첩 list 재인덱싱, 주요 list/dict 메서드, Float 인자·반환 closure/method가 no-VM exe에서 동작. exe-diff 42/0 유지 |
| W-2026-11-24-LANG5 | CLI 빌드 옵션(profile/opt-level) + 캐시 기초 | ARC-00 | LANG3(main.rs 충돌 피하기 위해 순차) | `main.rs`, `CLI.md` | `un build --opt-level N` 동작, 기존 출력 byte-identical |
| W-2026-11-24-LANG1 | Any/Union → no-VM 경로 | NRT-06 + CMP-05 | LANG4 완료(NRT 파일 충돌) | `nrt/**`, `typeck.rs`, `tir.rs`, `natcode.rs` | `Any`/Union 코드가 `--embed` 없이 빌드, 동적 dispatch 의미 보존 |
| W-2026-11-24-LANG2 | `go`/`wait` 네이티브 지원 또는 명시 대체 | NRT-06 + VM-02 | LANG1 완료 | `nrt/**`, `vm/**`, `runtime/**` | NRT의 단일 스레드 가정 제거 또는 지원 불가를 명시적 거부로 변경 |

Wave 1: LANG3, LANG4 병렬(파일 소유권 비충돌). Wave 2: LANG5(ARC-00가 main.rs 직접), LANG1. Wave 3: LANG2.

## 2. 공통 원칙

1. **`un run`과 byte-identical**: stdout/stderr/exit, traceback caret까지 유지. `exe-diff.ps1` 42/0이 회귀 게이트.
2. **ABI 신중**: 새 `unrt_*` 심볼은 ARC-00 승인, 기존 심볼 시그니처 변경 금지.
3. **`--embed`는 escape hatch**: 지원 범위를 넓히되 기존 동작을 깨지 않는다.
4. **검증**: build 0/0, workspace 254, test.ps1 50/1606, compile-check 36/0, exe-diff 42/0, strict canary 5종.

## 3. 보류/별도 RFC

- trait/interface, HKT, const generic — 문서/설계 선행(높은 난이도).
- 패키지 매니저, 크로스 컴파일, DAP/디버그 심볼 — 별도 생태계 Work.
- 메모리 소유권/arena 해제 — 거동 변경이라 별도 RFC.
