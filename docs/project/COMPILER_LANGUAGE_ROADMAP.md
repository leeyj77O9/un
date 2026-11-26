# COMPILER_LANGUAGE_ROADMAP.md — 컴파일 언어로서의 공백 해소 계획

> 작성: 2026-11-23, ARC-00. 근거: `docs/project/COMPILER_LANGUAGE_GAPS.md` 분석(에이전트 보고 취합).
> 이 문서는 계획이며, 구현은 Work 단위로 분배한다. 상태는 `.opencode/SESSION_STATUS.md`에 기록한다.
> 상태 갱신: 2026-11-24, DOC-01. 원본 `COMPILER_LANGUAGE_GAPS.md`는 저장소에 없다(ROADMAP 인용만
> 존재 — `docs/guides/COMPILED_LANGUAGE.md` 부록 B 항목 1도 동일하게 기록).

## 현재 기준 (2026-11-27, `0d98743`)

| 게이트 | 기준 |
|---|---|
| `cargo build --release` | 오류 0, 경고 0 |
| `cargo test --release --workspace` | **430 passed** (crate별: nrt 92 + nrt scheduler_spike 5 + lib 301 + un bin 24 + playground 8) |
| `scripts/test.ps1` | Positive **51 files / 1638 passed / 0 failed / 0 crashed**, Negative 51 matched, Known 0 |
| JIT differential | **102 files / 0 differences** |
| `.unbc` round-trip | Positive **51/51**, Negative **42 identical** (9 compile-rejected skip) |
| Compile Mode cross-check | **42/0** |
| exe-diff (no-VM 동치) | **78/0** |
| strict canary (`UV_JIT=2`) | **5/5** (Int / Float / collections / class / ops) |
| Un JIT / Python geomean | **3.688x** (2026-11-25 18:57:42 +09:00, `088973b` 기준) |

- **(각주 — 작성 시점 스냅샷 보존)** 2026-11-24/`5d7804c` 기준 이 표는 `cargo test` **315**, compile-check **40/0**, exe-diff **71/0**, geomean **4.428x**(2026-11-24 20:47, `ca47f3f`)였다.당시 "총계 314는 `5d7804c` 커밋 메시지와 일치한다"는 문구도 그 시점 기준이다. crate별 세부는 문서 간 차이가 있었다:
  `docs/README.ko.md`는 nrt 16 + lib 267 + LSP 24 + playground 8,
  `docs/project/REFACTOR_SUMMARY.md`는 nrt 16 + lib 267 + un bin/LSP 24 + playground 8로 적혀 있었다. 현재값은 위 표가 기준이다.

## 1. 우선순위와 파도(Wave)

| Work | 과제 | 담당 | 선행 | 허용 파일 | 완료 조건 |
|---|---|---|---|---|---|
| W-2026-11-24-LANG3 | 모듈 최상위 의미 확정(silent 무시 제거) | VM-02 + ARC-00(main.rs 통합) | 없음 | `module.rs`, `compiler.rs`, `vm/**`, `runner.rs` | **완료 (2026-11-24)**: 조용히 무시하는 경로 0. plain build가 `find_ignored_top_level_statement`로 첫 문장을 찾아 `top-level statements are not supported in plain build; move them into fn main() or use un build --embed` 진단 + exit 1(`87358f7`, `compiler.rs:48-91`, `main.rs:1061-1065`). `un run`/native/embed 의미 무변경 |
| W-2026-11-24-LANG4 | 컬렉션/Float closure 커버리지 | NRT-06 + CMP-05 + NAT-07 | 없음 | `nrt/**`, `typeck.rs`, `tir.rs`, `natcode.rs`, `unrt.rs`, `native/**` | **완료 (2026-11-24)**: 중첩 list `xs[0][1]` read/write, Float 인자·반환 closure/method(typed shim), `list.remove`, `dict.keys/values/clear`, `list.clear/resize`가 no-VM exe에서 동작. `func(..)` 파라미터·클래스 필드도 zero-capture wrapper로 지원(D-095 후속, `180d3db`/`c2138d7`/`e07b76d`, `COMPILED_LANGUAGE.md:216-217`). 새 NRT 심볼은 ARC-00 승인, 기존 ABI 유지 |
| W-2026-11-24-LANG5 | CLI 빌드 옵션(profile/opt-level) + 캐시 기초 | ARC-00 | LANG3(main.rs 충돌 피하기 위해 순차) | `main.rs`, `CLI.md` | **부분 완료 (2026-11-24; 당시 스냅샷)**: `--opt-level 0\|1\|2\|3\|s\|z`는 구현됨(`main.rs:832,850-860,960,1275`, `CLI.md:35,260`), 기본값 0으로 기존 출력 byte-identical. 당시 **캐시 기초(TOOL1)는 미구현**(`main.rs`에 빌드 캐시 없음)으로 적혔으나 **2026-11-27 현재 빌드 캐시 구현 완료** — 아래 §4 잔여 표도 갱신. 종전 '미구현' 표기는 opt-level 확인으로 정정 |
| W-2026-11-24-LANG1 | Any/Union → no-VM 경로 | NRT-06 + CMP-05 | LANG4 완료(NRT 파일 충돌) | `nrt/**`, `typeck.rs`, `tir.rs`, `natcode.rs` | **완료 (2026-11-24, 조건부)**: `75f538d`가 `nrt/dynops.rs`의 `unrt_dyn_bin/un`으로 동적 dispatch를 구현하고 `tir::nrt_supported`가 `TirTy::Obj`를 허용(`tir.rs:768-770,789`), Obj-mode plain build가 동적 함수를 emit(`tir.rs:1113-1117`). 단 `991acdd` 이후 기본은 fully typed mode라 명시적 `any`/Union은 거부 → **`--allow-dynamic` 필요**(`COMPILED_LANGUAGE.md:211-215`) |
| W-2026-11-24-LANG2 | `go`/`wait` 명시 거부 (안 B) | ARC-00 | LANG1 완료 | `compiler.rs`, `main.rs`, negative fixture | **완료 (2026-11-24)**: 호출되지 않는 함수까지 AST 감지 → 명시 오류. 실제 병렬(안 A)은 사용자 RFC 필요, D-012/D-054/D-063 유지. **(2026-11-27 갱신)** 이후 **안 C 구현 완료**(D-099, `un build --parallel` plain 전용 opt-in; P08 `488bf0b`) — 안 A는 미승인. 기본 경로는 여전히 명시 거부/deferred 유지. |

Wave 1(LANG3, LANG4) 완료. Wave 2: LANG1 완료, LANG5는 opt-level 완료 + **빌드 캐시 구현 완료(2026-11-27)**. Wave 3: LANG2 완료(명시 거부) → **안 C opt-in 병렬 구현 완료(D-099)**.

## 2. 공통 원칙

1. **`un run`과 byte-identical**: stdout/stderr/exit, traceback caret까지 유지. `exe-diff.ps1` **78/0**이 회귀 게이트(당시 71/0은 작성 시점 스냅샷).
2. **ABI 신중**: 새 `unrt_*` 심볼은 ARC-00 승인, 기존 심볼 시그니처 변경 금지.
3. **`--embed`는 escape hatch**: 지원 범위를 넓히되 기존 동작을 깨지 않는다.
4. **검증 (2026-11-27 실측)**: build 0/0, workspace **430**, test.ps1 51/1638, compile-check **42/0**, exe-diff **78/0**,
   differential 102/0, unbc 51/51+42, strict canary 5/5. (당시 314/41/75는 작성 시점 스냅샷.) 상세는 위 '현재 기준' 표 참조.

## 3. 보류/별도 RFC

- trait/interface, HKT, const generic — 문서/설계 선행(높은 난이도).
- 패키지 매니저, 크로스 컴파일, DAP/디버그 심볼 — 별도 생태계 Work.
- 메모리 소유권/arena 해제 — 거동 변경이라 별도 RFC.

## 4. 잔여 항목 (2026-11-24, DOC-01 교차 확인)

| 잔여 항목 | 근거 | 상태 |
|---|---|---|
| class/variant pattern AOT + named field | W-FIELDDESTRUCT 완료 — positional/named `TirPattern::Fields` 코드젠, 기존 unrt ABI 재사용 | 완료 |
| 함수 반환 타입 `-> func(...)` | W-FUNCRET + W-JITROOT 완료 — JIT 센티넬 유출 proto bail, 3레인 byte-identical | 완료 |
| 타입 패턴 상속 / 사용자 `__is__` | D-097 §4 — NRT는 tag 기반 정확 비교라 `--embed` 유지 필요(`ca47f3f` 커밋 메시지) | 미구현(plain) |
| `go`/`wait` 진짜 병렬(안 A) | LANG2는 안 B(명시 거부)만 완료; D-012/D-054/D-063 폐기·byte-identical 충돌로 RFC 대기. **안 C(opt-in)는 D-099로 구현 완료(P08 `488bf0b`), 안 A는 여전히 미승인** | RFC(안 A) / 안 C 완료 |
| CLI 빌드 캐시 기초 | LANG5의 opt-level은 완료, 당시 캐시(TOOL1) 미착수로 적혔음 | **구현 완료 (2026-11-27)** |
| TD-10 / TD-20 / TD-21 / TD-47 | `TECHNICAL_DEBT.md` — refcount-clone 잔여, frame/scope 중복, equality·clone 중복, os/sys helper 중복(won't-fix) | 저우선 잔여 |
| trait/interface, HKT, const generic / 패키지 매니저 / 크로스 컴파일 / DAP / arena 해제 | 위 §3 | 설계·생태계 보류 |

- 확인 필요: `native/bytes.rs`의 임시 `List<Int>` 표현을 `Payload::Bytes`로 교체하는 어댑터 3함수
  (AGENTS.md의 후속 지점). 원본 `COMPILER_LANGUAGE_GAPS.md` 부재로 초기 분석 근거는 ROADMAP 인용뿐이다.
  **(2026-11-27 각주)** 이 항목은 작성 시점(2026-11-24) 스냅샷이며 **D-098로 구현 완료**됐다.
  `ObjData`/`Payload::Bytes`를 VM·NRT 양쪽에 추가하고 어댑터 3함수(`bytes_obj`/`as_bytes`/`is_bytes`)가 새 표현을 우선 사용한다. `TECHNICAL_DEBT.md`의 대응 메모도 갱신됐다.
- 문서 간 불일치(본 문서 수정 범위 밖): `docs/guides/COMPILED_LANGUAGE.md:224`는 바이트 리터럴
  `b"..."`를 "거부"로 적지만 `26d457f` 이후 plain build가 지원하며
  `docs/compile/OBJ_MILESTONES.md:211`과 충돌한다. `docs/guides/CLI.md:271-273`와
  `OBJ_MILESTONES.md:259-261`은 동적 타입을 `--embed` 필요로 적지만 코드는 `--allow-dynamic`으로
  plain emit한다(`tir.rs:1113-1117`).
