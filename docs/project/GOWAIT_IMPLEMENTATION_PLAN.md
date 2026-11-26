# GOWAIT_IMPLEMENTATION_PLAN.md — `go`/`wait` opt-in 병렬 구현 Work 분해

> Work ID: W-2026-11-25-GOWAITPARALLEL / 역할: ARC-00 (Architecture / Integration Owner)
> 작성일: 2026-11-25 / 상태: **안 C 구현 반영 — P00~P08 구현·커밋 완료(P08 `488bf0b`), P09(문서 정리)만 잔여.** (HEAD `0d98743` 기준 확인)
> (각주: 최초 작성 시점 상태는 "설계·Work 분해만 완료. 코드·테스트·스크립트 변경 없음, 커밋 없음"이었다.)
> 인용된 `path:line`은 작성 시점(2026-11-25) 기준이며 현재 코드와 다를 수 있다 — 심볼명 기준으로 해석한다.
>
> 선행: `docs/project/GOWAIT_PARALLEL_RFC.md` 전체(특히 §5 추천안 C, §6 마일스톤, §7 체크리스트, §8 결론),
> `AGENTS.md` MANDATORY(규칙 1~8), `docs/OPENCODE_SESSION_SYSTEM.md` §3.1 소유권 / §8 통합·검증,
> `docs/project/DECISIONS.md` D-012 / D-054 / D-061 / D-062 / D-080(+ RFC §7.1이 요구하는 D-063) / D-099,
> `docs/project/TECHNICAL_DEBT.md` TD-39, `docs/project/PROJECT_AUDIT.md` §3,
> `docs/project/NATIVE_CONCURRENCY_DESIGN.md`, `docs/project/COMPILER_LANGUAGE_ROADMAP.md` LANG2.
>
> **승인 전제**: RFC §8의 "구현 착수 전 사용자 승인"은 사용자의 "남은거 전부 진행해" 지시로 갈음하였다.
> 단, **RFC 추천안 C(기본 deferred 유지 + opt-in 병렬)** 만 착수 대상으로 하고 안 A(기본 병렬 전환)는 채택하지 않는다.

---

## 0. 불변식 — 이 계획이 절대 건드리지 않는 것

AGENTS.md MANDATORY의 §8 지시(**cooperative concurrency는 의도된 계약이며 cleanup으로 고치지 말 것**)를
최상위 제약으로 둔다. 따라서 "deferred cooperative를 버그로 보고 고치는" 안 A는 설계 단계에서 배제한다.

| # | 불변식 | 근거 |
|---|---|---|
| I1 | 기본 `go`/`wait`는 D-012/D-054/D-063의 deferred cooperative inline이다. 값·캡처·순서·에러 문구가 변하지 않는다. | AGENTS.md §8, D-012/D-054/D-063, TD-39(Resolved by design) |
| I2 | 기본 경로의 byte-identical 게이트(`exe-diff`, `differential`, `test.ps1`, `compile-check`, `unbc-roundtrip`, `edge-fuzz`)는 **수치도 비교 방식도 바꾸지 않는다.** | RFC §5 "게이트 보존이 최우선", OPENCODE_SESSION_SYSTEM §8.3 |
| I3 | `Op` enum의 `MakeFuture=0x63` / `Await=0x64` 포함 모든 discriminant를 유지한다. `.unbc` 왕복 호환성 유지. | AGENTS.md, PROJECT_AUDIT §3, `bytecode/instr.rs` contract test |
| I4 | 기존 `unrt_*`/`nrt_*` 시그니처는 변경하지 않는다. 새 심볼만 추가하고 ARC-00 사전 승인을 받는다. | AGENTS.md MANDATORY 규칙 6, RFC §3.3 |
| I5 | .NET 유래 에러 문자열을 바꾸지 않는다(사용자 가시 변경, cleanup 아님). | PROJECT_AUDIT §3 |
| I6 | `main.un`(사용자 스크래치), `docs/audit/evidence/**`, 기존 untracked/수정 파일을 덮어쓰거나 정리하지 않는다. | AGENTS.md MANDATORY 규칙 7 |
| I7 | 병렬 수정은 세션별 별도 worktree에서만. 공유 `un.exe` 재빌드와 그 바이너리를 쓰는 테스트를 동시에 돌리지 않는다. | AGENTS.md MANDATORY 규칙 8, SESSION_SYSTEM §8.3 |
| I8 | 새 외부 crate(crossbeam 등)를 추가하지 않는다. `std::thread` + `Arc<Mutex>` + `Condvar`만 쓴다. | RFC §3.1, vendor 정책 |

---

## 1. opt-in 진입점 확정

### 1.1 결정

**`un build --parallel` (plain native exe 전용) 로 단일 확정한다.**

- `un run`(VM), `un native`(Vm 모드), `un build --embed`, `un check`는 **영향 없음**(기본 deferred 유지).
- 플래그가 붙은 plain build에서만 `go`/`wait`에 OS 스레드 풀 경로를 활성화한다.
- RFC §2 마지막 단서("안 C는 opt-in 경로를 네이티브(/plain build) 전용으로 두고 VM은 deferred로 유지하는 변형도 허용")를 정식안으로 채택한 것이다.

### 1.2 세 후보 비교와 선택 근거

| 후보 | 변경 표면 | 게이트 분리 난이도 | 판정 |
|---|---|---|---|
| `go!` (unary 변형) | Lexer/Parser/AST/Optimizer/Compiler (FE-01 + VM-02). `!`는 이미 `TokenType::Bang`이며 typeck.rs:3999 / tir.rs:5093에서 narrowing 의미로 쓰여 **문법 충돌·모호성 위험** | 표현식 단위라 기본 경로와 섞여 분리 어려움 | **보류** — 별도 FE-01 Work + 사용자 승인 필요 |
| `flow.parallel { }` | `flow` 모듈(NAT-07) + 블록/람다 인자 문법(FE-01). 기존에 `parallel` 심볼 없음 | 블록 단위라 중간 난이도 | **보류** — `go!`와 동일한 이유로 별도 승인 |
| **`un build --parallel`** | `main.rs:47 build_cmd` / `:65 help` (ARC-00 소유), TIR/natcode(CMP-05), NRT ABI(NRT-06). **Lexer/Parser/AST 무변경** | 빌드 단위라 완전 분리 가능. 기본 게이트 0개 변경 | **채택** |

선택 이유:

1. **게이트 보존(§0 I2)과 정확히 일치**: opt-in이 빌드 레벨이므로 `differential.ps1`은 애초에 병렬 레인을 볼 일이 없다(JIT와 직교). `exe-diff.ps1`도 기존 레인은 그대로 두고 새 레인만 추가하면 된다.
2. **public interface 변경 최소**: 문법 토큰을 새로 만들지 않는 대신 CLI 옵션 하나만 추가한다. CLI 옵션은 ARC-00 사전 승인 대상이므로(AGENTS.md 규칙 6) §6 체크리스트에 명시적으로 올린다.
3. **되돌리기 쉬움**: 플래그 제거 + 새 심볼 제거로 이전 상태 복귀. 언어 문법을 건드리지 않아 rollback 비용이 0에 가깝다.
4. **`--embed` 철학과 일관**: 이미 "dynamic/untyped면 embed로"라는 escape hatch가 있는 프로젝트에서 "병렬도 build 옵션으로"는 자연스러운 연장선이다.

### 1.3 보류 항목 재확인 필요

- `go!` / `flow.parallel { }`는 **이 Work 범위에서 제외**한다. 사용자가 "표현식 단위 opt-in까지 필요하다"고 명시하면 FE-01 소유의 별도 Work(ID 신규 발급)로 분리한다. §6 체크리스트 항목 C-1.

---

## 2. Work 분해 표

세션 ID는 `OPENCODE_SESSION_SYSTEM.md` §2의 표준 역할(FE-01/VM-02/RT-03/JIT-04/CMP-05/NRT-06/NAT-07/TOOL-08/QA-09/BENCH-10)을 따른다.
한 파일에는 주 소유자만 두고(§3.1), 공동 관리 파일은 ARC-00 승인 없이 수정하지 않는다.

| Work ID | RFC M | 담당 세션 | 소유 파일(신규 포함) | 선행 조건 | byte-identical 게이트 영향 | 완료 조건 | 공수(인일) |
|---|---|---|---|---|---|---|---|
| **W-25-P00** | M0 | ARC-00 | 본 문서(`docs/project/GOWAIT_IMPLEMENTATION_PLAN.md`). 승인 후에만 `docs/project/DECISIONS.md` | 없음 | 없음(문서 작업) | §6 재확인 체크리스트 승인 + 세션별 Work ID 발급 + RFC §7.3 최종 확정 | 1.0 |
| **W-25-P01** | M1 | ARC-00 주재 + NRT-06/RT-03 공동 검토 | 신규 `docs/project/GOWAIT_THREAD_SAFETY_AUDIT.md` (문서). 코드 변경 없음 | P00 | 없음(정적 감사) | RFC §2의 11행 전부에 "기본 경로 / Send 경계 / opt-in 적용 여부" 판정 기록, 미해결 `static mut`/`&'static mut` 0건, 서명 완료 | 1.5 |
| **W-25-P02** | M2a | NRT-06 | `rust/nrt/src/{class.rs, obj.rs, list.rs, dict.rs}` | P01 | `exe-diff` 기본 레인 78/0(수치 재확인) 무변경 필수 | `static mut` 3개 제거, bump arena → `Mutex<Vec<u64>>` + `AtomicUsize`, `instance_fields()` 핸들화, C ABI는 `u64` 유지, `cargo build --release` 0 error/0 warning | 3.0 |
| **W-25-P03** | M2b | NRT-06 | `rust/nrt/src/{globals.rs, closure.rs, abi.rs}` | P01 | 기존 `unrt_*` 시그니처 변경 0건 | capture 값의 `Send` 보장 명시, 함수 포인터 transmute 전에 callee의 정적 가변 접근이 P02에서 해소되었음 확인, 기존 심볼 시그니처 동일 | 2.0 |
| **W-25-P04** | M3 | NRT-06 | `rust/nrt/src/trace.rs` (+ task-local 결과 버퍼 타입) | P02, P03 | traceback byte-identical 기본 레인 유지(에러 문구 I5) | `FRAME_DEPTH`/`FRAME_SLOTS`/`ERROR_SITE` 전역 정적 제거, task-local 수집 → `unrt_wait` 결과에 trace 포함 → wait 스레드에서 직렬 출력 | 2.5 |
| **W-25-P05** | M4 | NRT-06 | 신규 `rust/nrt/src/scheduler.rs` (단일 공유 큐) | P01 감사 완료. P02와 병렬 착수 가능(아래 P05s) | 기본 레인 무영향(새 모듈만 추가) | `std::thread::Builder`+`Arc<Mutex<VecDeque<Task>>>`+`Condvar`, graceful drain+join, `close()` 후 submit은 기존 `"pool is closed"` 유지, 자기 참조 wait의 inline fallback, 스파이크 단위 테스트 통과 | 2.5 |
| **W-25-P06** | M5 | CMP-05 주 + ARC-00(`main.rs`) + NRT-06(ABI 등록) | `rust/src/{tir.rs, natcode.rs, unrt.rs, main.rs}`, `rust/nrt/src/{abi.rs, native.rs}` | P02~P05 | `unbc-roundtrip` / `compile-check` / strict canary 기본 무변경, opcode 0x63/0x64 유지(I3) | `Go`/`Wait` TIR variant + `expr_ok`/`stmt_ok`, `un build --parallel` 플래그(help 포함), 새 `unrt_*` 심볼 3개 등록, **기본 경로는 기존 deferred 분기로 계속 drop**, LANG2 거부를 opt-in에서만 해제 | 3.0 |
| **W-25-P07** | M6 | NAT-07 주 + NRT-06 공동 | `rust/src/native/{flow.rs, atomic.rs}`, `rust/nrt/src/native.rs` | P05, P06 | `test/unit/runtime/async.un` **기대값 수정 금지**(I1; RFC §4의 타이밍 갱신 제안은 안 C에서 채택하지 않음) | opt-in에서만 실제 스레드 실행, `pool.map` 입력 순서 보장, `atomic`을 `AtomicI64`/`AtomicBool`로 재정의하되 값·`exchange`·`compare_and_swap`·`Clone`/`Copy`/`Entry`/`Exit` **오류 문구 동일** | 2.0 |
| **W-25-P08** | M7 | QA-09 주 + TOOL-08(CI 스텝) | 신규 `scripts/gowait-parallel.ps1`, 신규 `test/parallel/manifest.txt`, `.github/workflows/build.yml` | P06, P07 | **기존 8개 게이트 스크립트의 default 분기 0개 변경**, 수치 유지. 새 게이트는 별도 | 새 게이트가 집합(`RESULT|`) 비교 + exit code + stderr 문구 검증, opt-in 케이스만 격리, `.github/workflows/build.yml`의 `Test (go/wait opt-in parallel gate)` 스텝 추가 완료(`488bf0b`; 스텝 번호는 파일 참조, 고정 숫자 없음), 기존 스텝 전부 green 유지 | 1.5 |
| **W-25-P09** | M8 | ARC-00 | `README.md`, `docs/README.ko.md`, `docs/spec/ARCHITECTURE.md`, `docs/guides/{CLI.md, CLI.ko.md, COMPILED_LANGUAGE.md, BENCHMARK_GUIDE.md}`, `docs/project/DECISIONS.md`(승인 문구 반영), `docs/project/TECHNICAL_DEBT.md` TD-39 / TD-54~TD-56 각주 | P08(`488bf0b`) + 사용자 승인 | 문서 기준 수치는 실행 시점과 명령 함께 명시(SESSION_SYSTEM §9-10) | 한/영 쌍 동기, "기본 deferred + opt-in 예외" 문구 일관, 옵션 제거로 되돌리는 방법 명시 | 1.0 |

> **Work 상태 (2026-11-27 갱신, HEAD `0d98743`)**: **P00 완료**(설계 문서 산출물, 별도 커밋 없음) · **P01 완료**(감사 산출물) · **P02 완료**(`3ae916a`, P02B 포함) · **P03 완료**(`a9d1d24`) · **P04 완료**(`d2f176c`) · **P05 완료**(`9d63c7f`; scheduler 스파이크 포함) · **P06 완료**(`a7b39ea` / `fc7e5f8` / `baaefba`) · **P07 완료**(`013858a` / `1b258b0`) · **P08 완료**(`488bf0b`; `scripts/gowait-parallel.ps1` + `test/parallel/manifest.txt` + `.github/workflows/build.yml`의 `Test (go/wait opt-in parallel gate)` 스텝) · **P09 잔여**(문서 정리; 본 Work W-2026-11-27-001 그룹 C가 일부 수행 — 아래 §3.5의 opt-in divergence 3건 포함). 표의 공수·완료 조건은 작성 시점 추정치로 보존한다.

**합계 20.0 인일** (RFC 안 C 밴드 **16–22 인일**의 중앙값; W-00~W-09의 합). 리뷰/CI 시간 제외, 1인 기준.

### 2.1 의존 그래프와 병렬 실행 경계

```
P00 → P01 ─┬─→ P02 ─┬─→ P04 ─→ P06 ─→ P07 ─→ P08 ─→ P09
           ├─→ P03 ─┘            ↑
           └─→ P05 ──────────────┘
                (P05s 스파이크는 P01 없이도 착수 가능)
```

- 동시 착수 가능: P02 ∥ P03 ∥ P05(각자 별도 worktree, 파일 소유 겹침 없음).
- `P06`은 세 갈래가 모두 모인 뒤에만 시작(ABI 경계가 한 지점에 모이므로 ARC-00이 통합).
- `rust/src/main.rs`는 ARC-00 소유(공동: VM-02/CMP-05/NRT-06/TOOL-08). P06의 CLI 부분은 **반드시 별도 승인된 ARC-00 구현 Work**에서 수행하고, 이 세션(설계 전담)은 직접 수정하지 않는다.
- `rust/src/tir.rs`는 CMP-05 소유이며 ARC-00이 공동 관리자다. 작업 디렉터리에 이미 수정 상태로 존재하므로(§7 Git 상태) 소유자 확인 없이 덮어쓰지 않는다.

### 2.2 각 Work의 "게이트 영향" 해석 규칙

- **"무변경"**은 문자 그대로 스크립트 파일의 default 분기를 수정하지 않는다는 뜻이다. 스크립트에 새 `-Group parallel` 분기를 **추가하는 것도 P08에서만** 허용하고, 기본 호출 경로의 기대값(102/0, 78/0, 1638 passed 등)은 절대 낮추지 않는다.
- **"새 게이트"**는 기존 스크립트를 약화한 모드로 재호출하는 것이 아니라 **독립 스크립트 + 독립 manifest**로 비교 강도를 별도로 정의한다(§3).

---

## 3. 게이트 재정의 범위

### 3.1 원칙

> **기본 경로는 기존 byte-identical comparison을 그대로 유지하고, 병렬(opt-in)은 별도 게이트로 분리한다.**

안 A에서 요구됐던 "stdout 순서 정규화", "wait 순서로 결과 재배치", "비결정 fixture 이동"은 **이 계획에서 전부 하지 않는다.**
기본 경로의 비교 강도를 낮추는 순간 회귀 탐지력이 사라지므로, opt-in 케이스는 아예 다른 manifest·다른 스크립트로 격리한다.

### 3.2 스크립트별 변경표

| 스크립트 / 게이트 | 현재 기준(실행 전 재확인) | 안 C에서의 변경 | 담당 |
|---|---|---|---|
| `scripts/differential.ps1` | 102 파일 / 0 차이 (JIT on/off byte 비교) | **변경 없음.** 병렬 opt-in은 JIT 경로가 아니므로 비교 대상이 아님. 단, "병렬 케이스는 이 게이트에 등록하지 않는다"는 주석만 P09 문서에 명시(스크립트 수정 아님) | QA-09(무변경) |
| `scripts/exe-diff.ps1` | 78/0 (AGENTS.md 기준; RFC는 71/0으로 표기 → **실행 시 재확인**) | **기본 레인 무변경.** `--parallel` 빌드용 별도 manifest를 새 게이트에서만 사용 | QA-09(무변경) |
| `scripts/test.ps1` | Positive 51 files / 1638 passed / 0 failed / 0 crashed | **기본 manifest 무변경.** `async.un`의 타이밍·정렬 기대값은 수정하지 않는다. opt-in 테스트는 `test/parallel/manifest.txt`로 분리 | QA-09(무변경) |
| `scripts/compile-check.ps1` | 42/0 | 변경 없음. 단, `Go`/`Wait` TIR variant 추가가 기본 타입체크 거부(LANG2)를 깨뜨리지 않는지 확인만 | QA-09(무변경) |
| `scripts/unbc-roundtrip.ps1 -IncludeNegative` | Positive 51/51 + Negative 42 identical(9 skip) | 변경 없음. opcode 0x63/0x64 유지(I3) 확인 | QA-09(무변경) |
| `scripts/edge-fuzz.ps1` | 5레인 크로스 언어 `RESULT|` 비교 | **기본 5레인 무변경.** 병렬 레인은 새 게이트의 별도 케이스로만 추가 | QA-09(무변경) |
| strict canary (`UV_JIT=2`) | 3스텝, 신규 task wrapper가 fallback stub 되면 exit 2 | 변경 없음. 새 wrapper가 JIT 대상 밖임을 확인하는 수준(무영향 확인만) | QA-09(무변경) |
| `cargo test --release --workspace` | **430 passed (2026-11-27 실측)**; 과거 표기 352(AGENTS.md 당시)/314(RFC 당시)는 작성 시점 스냅샷 | 변경 없음, 새 경고 0 유지 | 각 소유 세션 |
| **`scripts/gowait-parallel.ps1` (신규)** | 없음 | opt-in 전용: `un build --parallel` 산출 exe를 `test/parallel/manifest.txt`에 따라 실행 | QA-09 |
| `.github/workflows/build.yml` | **스텝 수는 고정하지 않고 파일 자체를 기준으로 함** | `.github/workflows/build.yml`의 `Test (go/wait opt-in parallel gate)` 스텝 추가 완료(`488bf0b`); 기존 스텝 하나도 약화·삭제 금지 | TOOL-08 + ARC-00 |

> 기준 수치의 71/0 ↔ 78/0, 314/352 ↔ 430 불일치는 문서 간 시점 차이다. OPENCODE_SESSION_SYSTEM §8.3 지시에 따라
> **실제 게이트 실행 전 스크립트와 최신 AGENTS.md에서 다시 읽어 확정**한다. 위 표의 "현재" 열만 2026-11-27 실측값으로 갱신했고, 과거 수치는 각주로 보존했다(이 계획의 원래 숫자는 바꾸지 않음).

### 3.3 신규 게이트 `scripts/gowait-parallel.ps1` 설계

- **비교 방식**: raw byte 비교가 아니라 **정규화된 결과 비교**를 사용한다.
  1. 각 케이스의 stdout에서 `RESULT|case|value` 줄만 추출하고 `case` 기준으로 정렬해 **multiset 비교**.
  2. exit code는 엄격 비교(기대 exit와 일치).
  3. stderr는 **에러 문자열을 그대로 엄격 비교**(I5). 비결정 trace 프레임 주소 등은 task-local trace 직렬화로 제거되어 있어야 통과.
  4. **실행 시간·태스크 시작 순서·stdout 직접 print 순서는 기대값에 포함하지 않는다.** 대신 `go` body 안의 직접 print는 진단 대상으로 manifest에 명시한다(RFC §3.2).
- **manifest 형식** (QA-09 소유, `test/MANIFEST.txt`의 `group|path` 관례와 별개):
  `path|expected_exit|expected_result_count` — `test/parallel/` 아래에 둔다.
- **격리 규칙**: 이 스크립트는 `scripts/exe-diff.ps1`을 "순서 무시 모드"로 호출하지 않는다. 기존 스크립트를 재사용하지 않고 독립 구현한다.
- **실패 조건**: `RESULT|` 불일치, exit 불일치, 에러 문구 불일치, 미해결 task(드레인 실패), 데드락 타임아웃.
- **성공 조건**: manifest 전 케이스 통과 + 기본 게이트 8종이 그대로 green.

### 3.4 벤치마크

`scripts/bench.ps1`의 6레인과 `BENCHMARK.md` geomean은 **현재 정의를 유지**한다. 병렬 측정은 `-Group parallel` 별도 그룹으로 추가하고, BENCH-10이 단일 소유자로 리포트를 생성한다(SESSION_SYSTEM §9-11). 비결정 병렬 레인을 기존 geomean에 섞지 않는다.

### 3.5 opt-in 경로의 documented divergence와 잠재 이슈 (2026-11-27 관측, P09 문서화 잔여)

P08(`488bf0b`) 완료 후 opt-in 전용 게이트가 관찰한 차이를 여기에 기록한다. **기본 deferred 경로와 byte-identical 게이트는 변하지 않는다.** 상세 항목은 `docs/project/TECHNICAL_DEBT.md` TD-54 / TD-55 / TD-56에 낮은 심각도로 등록했다.

1. **TD-54 — future 결과 조회 API 차이 (documented divergence)**: 병렬 plain exe의 `pool.submit` future는 **`.value()`만 지원**하고, VM(`un run`)은 **`wait`만 지원**한다. opt-in 경로에서 `wait`를 쓰는 프로그램은 기본 경로와 동일하게 동작하지 않으므로, `--parallel` 문서화 시 이 차이를 명시해야 한다(P09 잔여).
2. **TD-55 — task 내부 throw의 traceback 프레임 차이 (documented divergence)**: 병렬 task 안에서 throw된 예외는 `wait` 지점의 **error 메시지가 동일**하지만, VM의 traceback 프레임(`-> <module>`, `-> go`)이 **누락될 수 있다**. `scripts/gowait-parallel.ps1` 게이트는 error 문구만 검증하므로 프레임 차이는 회귀 실패로 잡히지 않는다(P09 잔여).
3. **TD-56 — 스케줄러 lost-wakeup 가능성 (잠재 이슈, 확정 버그 아님)**: `rust/nrt/src/scheduler.rs`의 `cv.notify_all()`이 shared lock 없이 호출되고 `join_full`에 watchdog이 없다. **1회 30초 타임아웃이 관측됐으나 이후 100회 재현 시 재발하지 않았다.** 따라서 재현 조건이 확정된 버그가 아니라 잠재 이슈로만 기록한다(P09 잔여; 수정은 별도 Work/사용자 승인 대상 — 이번 작업은 문서화만).

---

## 4. 의사결정 개정 초안 (승인용 — **지금 적용하지 않음**)

아래는 `docs/project/DECISIONS.md`에 **나중에 별도 Work로追加**할 문구의 초안이다.
기존 조항은 삭제·폐기하지 않고 **"기본은 유지 + opt-in 예외 추가"** 형식으로만 덧붙인다(PROJECT_AUDIT §3: decisions are marked superseded, never deleted — 여기서는 supersede조차 하지 않고 amendment만 한다).

### D-012 — `go`/`wait`는 single-threaded cooperative scheduler

> **개정 초안 (W-2026-11-25-GOWAITPARALLEL, 안 C)**:
> 기본 경로는 본 결정과 D-054/D-063의 single-threaded cooperative/deferred 계약을 그대로 유지한다.
> `un build --parallel`로 빌드한 plain native exe에 한해 `go`/`wait`가 OS 스레드 풀에서 실행되는 예외를 둔다.
> 이 예외는 언어 기본 semantics의 변경이 아니라 **빌드 옵트인**이며, 플래그 제거로 완전히 되돌릴 수 있다.
> "semantic 차이 보고 항목"(블로킹 네이티브가 전체를 멈춤)은 기본 경로에서 계속 유효하고, opt-in 경로에서는
> 문서화된 차이(documented divergence)로 취급한다.

### D-054 — `go`/`wait`과 pool은 호출 스레드에서 실행한다

> **개정 초안**:
> `Rc<RefCell<..>>` 객체 모델과 호출 스레드 실행은 **기본 경로의 계약**으로 유지한다.
> opt-in 경로에서만 `Send` 전용 snapshot과 `Arc<Mutex>` 기반 실행기를 사용하며, VM(`un run`)과
> `un build --embed` 경로는 변경하지 않는다. "결과 값은 같지만 인터리빙은 다르다"는 기존 서술은 기본 경로에 적용되고,
> opt-in 경로에서는 인터리빙이 관측 가능하므로 `pool.map`의 입력 순서 보장 계약(RFC §3.2)을 추가한다.

### D-061 — `[Native(Async = true)]`는 binder가 무시하고, `f43637f`의 dead-code 정리는 Rust에 영향이 없다

> **개정 초안**:
> opt-in 병렬 경로는 binder의 `Async` 어트리뷰트와 **무관하게** 유지된다. `_async` 멤버는 여전히 blocking 멤버와
> 동일하게 동작하고, 새 스케줄러는 언어 레벨의 `go`/`wait`와 `flow`/`atomic`에만 연결된다.
> 따라서 `f43637f`가 지운 항목 목록은 그대로 유효하며, "async 네이티브를 복원해 병렬을 얻는" 접근은 채택하지 않는다.
> 즉 이 결정은 병렬화의 장애물이 아니라 **cleanup으로 고치지 말아야 할 의도된 계약**으로 남는다.

### D-062 — `atomic`의 값은 한 cell에 담는다

> **개정 초안**:
> 기본 경로에서 counter/flag는 `Rc<RefCell<i64>>`/`Rc<RefCell<bool>>` 한 cell에 담는다(값 동일).
> opt-in 경로에서만 `AtomicI64`/`AtomicBool` 한 cell로 재정의한다. `add`/`inc`/`dec`의 wrapping 동작,
> `exchange`가 이전 값을 반환하는 규칙, `compare_and_swap`의 조건부 swap, `Clone()` 거부 / `Copy()`가 `this`를
> 반환하는 규칙, `Entry()`/`Exit()`의 에러 문구는 **두 경로 모두에서 동일하다.**

### D-080 — `go` fork-time scope snapshot 유지

> **개정 초안**:
> 기본 경로의 fork-time deep snapshot(capture-by-value) 계약과 "200,000 forks ≈ 1,058 ms로 병목이 아님"이라는
> 근거는 그대로 유지한다. opt-in 경로는 `Send` 가능한 스냅샷으로 전달하되 **캡처-by-value 의미론은 동일**하게
> 유지하고, value cell 공유분만 복사로 전환한다(공유 cell은 레이스 지점이므로). 병목 측정 근거는 기본 경로
> 기준으로 유효하며, opt-in 경로는 별도 벤치마크 그룹에서 측정한다.

### (참고) D-063 — `go`는 `go` 시점의 scope chain 복사본을 가진 deferred future다

> RFC §7.1이 D-012/D-054/D-062/D-080과 함께 개정을 요구한다. 위 목록에 없었지만 같은 semantics 계약의
> 일부이므로 초안을 함께 적어 둔다:
> **기본 경로는 chain 복사본 + `wait` 시점 child scope 실행 + 결과 memoization을 유지한다.**
> opt-in 경로는 복사본을 `Send` 가능한 형태로 전달할 뿐, "loop 안 `go`가 각 iteration 값을 캡처한다"는
> 관측 동작(`test/unit/runtime/async.un`)과 동일 future 재`wait`의 memoization은 그대로 보장한다.

### LANG2 완료 조건

`docs/project/COMPILER_LANGUAGE_ROADMAP.md`의 "안 B 완료 = AST 전체에서 `Go`/`Wait` 검출 → 명시 거부"를
**"기본은 명시 거부 유지, `un build --parallel`에서만 허용"** 으로 갱신한다(`compiler.rs:261-290`의
`find_unsupported_async_node()` 거부 메시지 자체는 기본 경로에서 유지).

---

## 5. 공수 분해와 첫 슬라이스 추천

### 5.1 공수 배분 (RFC §6 M0~M8을 Work 단위로 재배분)

| RFC M | 인일 | 대응 Work | 비고 |
|---|---|---|---|
| M0 | 1.0 | P00 | 본 보고서로 산출물 완료(승인 대기만 남음) |
| M1 | 1.5 | P01 | **게이트화된 감사** — M2의 unsafe 누락 한 곳이 crate 안전성을 무효화하므로 선행 필수 |
| M2 | 5.0 | P02(3.0) + P03(2.0) | 객체 모델 / Send 경계 분리 |
| M3 | 2.5 | P04 | traceback 직렬화(바이트 동치의 핵심) |
| M4 | 2.5 | P05 | 스케줄러 코어(그중 0.5 인일 = 스파이크) |
| M5 | 3.0 | P06 | TIR/natcode/CLI/ABI 통합, ARC-00이 단일 통합점 |
| M6 | 2.0 | P07 | `flow`/`atomic` semantics + D-062 amendment |
| M7 | 1.5 | P08 | 게이트 분리(기본 스크립트 수정 없음) |
| M8 | 1.0 | P09 | 문서 동기화 |
| **합계** | **20.0** | P00~P09 | 안 C 밴드 16–22 인일 내 |

### 5.2 첫 슬라이스 추천

**추천: `W-25-P00`은 본 문서로 완료로 보고, 첫 착수 슬라이스는 `W-25-P01`(무코드 감사) →
`W-25-P05s`(스케줄러 스파이크, P05의 앞 0.5 인일) 로 잡는다.**

- **P01을 먼저 하는 이유**: RFC §6이 명시적으로 "M2의 `unsafe` 제거 누락 한 곳이 전체 crate 안전성을
  무효화하므로 M1 감사를 게이트화한다"고 못 박았다. §2의 11행(static mut, arena, `&'static mut`, WORLD,
  trace, abi 등)을 문서로 판정하지 않고는 어떤 `rust/nrt/**` 코드도 안전하게 만질 수 없다.
  비용 1.5 인일로 가장 큰 리스크를 먼저 제거한다.
- **그다음 P05s인 이유**: 사용자 재확인 항목 중 "crossbeam 없이 `std::thread`만 가능한가"(§6 C-3)는
  감사만으로는 답이 나오지 않는다. `rust/nrt/src/scheduler.rs`에 trivial `extern "C"` 태스크 2개를 넣고
  큐·Condvar·drain/join만 검증하는 스파이크는 **다른 세션 소유 파일을 전혀 건드리지 않고** 그 가정을
  실측으로 확인한다. 실패하면 §6 C-3을 다시 사용자에게 되돌릴 수 있는 지점이기도 하다.
- **의도적으로 뒤로 미루는 것**: P02(객체 모델)와 P06(ABI/CLI)은 감사와 스파이크가 끝난 뒤에 시작한다.
  특히 P06의 `main.rs`는 ARC-00 소유이므로 별도 승인 Work 없이 착수하지 않는다.
- **슬라이스 완료 판정**(SESSION_SYSTEM §8.2): P01은 `review` 상태로 끝나도 되지만, P05s는 스파이크
  단위 테스트를 실제로 실행한 뒤에만 `review`를 선언한다. 테스트를 못 돌렸으면 통과 보고 금지.

---

## 6. 사용자 재확인이 필요한 항목 체크리스트

> RFC §7.3을 이 계획의 확정안 기준으로 갱신한 것이다. 미승인 항목이 하나라도 남으면 해당 Work는 `blocked`로 보고한다(§8.2).
> **(2026-11-27 갱신)** 아래 12항목은 사용자 승인 갈음 + 구현 완료로 `[x]` 처리했다. `[x]`는 "승인되고 구현까지 반영됨"을 뜻하며, 미승인으로 확인한 항목(C-12)도 확인 완료 의미로 `[x]`다. 각주로 보존한 당시 표기는 `[ ]`였다.

- [x] **C-1 진입점 확정**: `un build --parallel` 단일 채택을 승인하는가. `go!` / `flow.parallel { }`는 이번 범위에서 보류해도 되는가. (보류 해제 시 FE-01 별도 Work) — **승인·구현**: D-099가 단일 진입점으로 확정, P06(`a7b39ea`/`fc7e5f8`/`baaefba`)이 CLI·TIR·ABI 반영. `go!`/`flow.parallel { }` 보류 유지.
- [x] **C-2 opt-in 범위**: plain native exe에만 적용하고 `un run` / `un native` / `--embed` / `un check`는 기본 deferred로 두는 것을 승인하는가. — **승인·구현**: D-099가 "plain 전용 opt-in, `--embed` 상호배제"로 확정, P06/P07이 기본 deferred 분기를 그대로 유지.
- [x] **C-3 의존성**: crossbeam 미도입, `std::thread` + `Arc<Mutex>` + `Condvar`만 사용하는 방안을 승인하는가(§5.2 스파이크로 실측 예정). — **승인·구현**: P05(`9d63c7f`)가 `rust/nrt/src/scheduler.rs`에 `std::thread` + `Arc<Mutex<VecDeque>>` + `Condvar`로 구현, 외부 crate 추가 없음.
- [x] **C-4 새 ABI 심볼**: `unrt_go` / `unrt_wait` / `unrt_trace`(초안 이름·시그니처) 3개 추가를 승인하는가. 기존 `unrt_*` 시그니처 변경은 금지 유지. — **승인·구현**: D-099가 새 C 심볼 3개를 명시 승인, P06이 NRT ABI에 등록. 기존 시그니처 변경 0건.
- [x] **C-5 CLI 옵션**: `--parallel` 추가(및 help 갱신)를 승인하는가 — AGENTS.md MANDATORY 규칙 6의 사전 승인 대상. — **승인·구현**: D-099에 사전 승인 기록, P06에서 옵션·help 반영.
- [x] **C-6 결정 개정 문구**: §4의 D-012/D-054/D-061/D-062/D-080 + D-063 + LANG2 amendment 초안을 승인하는가(폐기/Supersede가 아닌 追加임을 확인). — **승인·구현**: 초안대로 기존 조항을 수정·폐기하지 않고 **D-099 별도 append**로 갈음. D-012/D-054/D-061/D-062/D-063/D-080은 그대로 유지.
- [x] **C-7 게이트 재정의 범위**: §3의 "기본 8게이트 무변경 + 신규 `gowait-parallel.ps1` only" 안을 승인하는가. 특히 기존 byte-identical 비교 강도를 **낮추지 않는다**는 점을 확인. — **승인·구현**: 기본 게이트 실측 430 / 51 files·1638 / 42-0 / 78-0으로 강도 유지, 신규 게이트는 P08(`488bf0b`)에서 별도 완료. 단 opt-in 전용 차이는 §3.5 / `TECHNICAL_DEBT.md` TD-54~TD-56에 문서화(게이트는 error 문구만 검증).
- [x] **C-8 기존 테스트 기대값**: `test/unit/runtime/async.un`의 타이밍/정렬 기대값을 **수정하지 않는다**는 방침을 승인하는가(RFC §4의 갱신 제안은 안 C에서 미채택). — **승인·구현**: P07(`013858a`/`1b258b0`)에서 기대값 미수정 확인.
- [x] **C-9 공수·순서**: 20.0 인일(밴드 16–22)과 P00→P01→{P02,P03,P05}→P04→P06→P07→P08→P09 순서, 첫 슬라이스(P01→P05s)를 승인하는가. — **승인·구현**: 승인된 순서대로 P00~P08 완료, P09 잔여.
- [x] **C-10 worktree/Work ID 발급**: P02~P09를 별도 worktree에 분배하고 각 세션에 하위 Work ID를 발급해 주는가(§0 I7, 모두 커밋 없이 `review` 상태로 유지). — **승인·구현**: 세션별 worktree 분배 및 하위 Work ID 발급 완료(이 문서의 최초 `review` 표기는 작성 시점 기준으로 보존).
- [x] **C-11 기존 변경 보존**: 작업 디렉터리에 이미 수정 상태인 `AGENTS.md`, `rust/nrt/src/native.rs`, `rust/src/tir.rs`(특히 tir.rs는 CMP-05/ARC-00 공동 소유)를 이 작업의 일부로 간주하지 않고 **덮어쓰지 않도록** 소유자 확인을 해 주는가(§0 I6). — **승인·구현**: 선행 변경을 덮어쓰지 않고 소유자 확인 후 작업 분배.
- [x] **C-12 안 A 전환 여부**: 사용자가 "기본을 진짜 병렬로 바꿔도 된다"는 별도 승인을 낼 경우에만 §3의 비교 완화가 전면 재정의 대상이 됨을 확인한다(현재는 미승인 상태). — **확인 완료: 안 A 미채택 그대로 유지**, D-099도 기본 deferred 유지만 명시. `[x]`는 미승인 사실을 확인했다는 뜻이며 안 A를 수락했다는 뜻이 아니다.

---

## 7. 보고 요약 (SESSION_SYSTEM §2.3)

| 항목 | 내용 |
|---|---|
| **Work ID** | W-2026-11-25-GOWAITPARALLEL |
| **역할** | ARC-00 (Architecture / Integration Owner) — 설계·Work 분해 수행 + (2026-11-27 현재) 구현 통합 상태 반영 |
| **결과** | RFC 추천안 C 준수. opt-in 진입점 `un build --parallel` 확정. Work 10개(P00~P09) 분해, 게이트 분리안, 결정 개정 초안 5+1건, 공수 20.0 인일, 첫 슬라이스 P01→P05s 도출. **(2026-11-27 갱신)** P00~P08 구현·커밋 완료, D-099 append, P09 잔여. 안 A 미채택 유지. |
| **작성 파일** | `docs/project/GOWAIT_IMPLEMENTATION_PLAN.md` **1개만 신규 작성** (기존 문서 수정 없음, 코드/테스트/스크립트 수정 없음) — 최초 작성 시점 기준각주; 현재는 상태·체크리스트·링크를 갱신했다. |
| **Work 분해** | P00 M0/ARC-00 1.0 · P01 M1/ARC-00+NRT-06+RT-03 1.5 · P02 M2a/NRT-06 3.0 · P03 M2b/NRT-06 2.0 · P04 M3/NRT-06 2.5 · P05 M4/NRT-06 2.5 · P06 M5/CMP-05+ARC-00+NRT-06 3.0 · P07 M6/NAT-07+NRT-06 2.0 · P08 M7/QA-09+TOOL-08 1.5 · P09 M8/ARC-00 1.0 |
| **게이트 재정의안** | 기본 8게이트( differential 102/0, exe-diff 78/0, test 51 files/1638, compile-check 42/0, unbc-roundtrip 51/51+42, edge-fuzz 5레인, strict canary, cargo test ) **전부 무변경**. 신규 `scripts/gowait-parallel.ps1` + `test/parallel/manifest.txt` + `.github/workflows/build.yml`의 `Test (go/wait opt-in parallel gate)` 스텝(`488bf0b`)만 추가. 비교는 `RESULT|` 집합 + exit + 에러 문구, 시간/시작순서 제외 |
| **첫 슬라이스 추천** | P00은 본 보고서로 완료 간주 → **W-25-P01(무코드 Send 안전성 감사, 1.5 인일)** → **W-25-P05s(스케줄러 스파이크, 0.5 인일)**. P02/P03/P05는 별도 worktree에서 병렬 가능 |
| **사용자 재확인** | C-1~C-12 (진입점, opt-in 범위, std 전용, ABI 3개, CLI 옵션, 결정 개정 문구, 게이트 범위, 테스트 기대값 무수정, 공수·순서, Work ID 발급, 기존 변경 보존, 안 A 미승인) |
| **Git 상태** | 최초 작성 시점: **커밋 없음.** 선행 수정 3건(`M AGENTS.md`, `M rust/nrt/src/native.rs`, `M rust/src/tir.rs`) 존재, 일체 건드리지 않음, 신규 파일만 추가(`?? docs/project/GOWAIT_IMPLEMENTATION_PLAN.md`). **(2026-11-27 현재)** P00~P08 구현이 각각 커밋됨(`9d63c7f`, `3ae916a`, `a9d1d24`, `d2f176c`, `a7b39ea`/`fc7e5f8`/`baaefba`, `013858a`/`1b258b0`, `488bf0b`); 본 문서-갱신 작업에서는 커밋 없음. |
| **완료 판정** | 최초 작성 시점: `review` (설계 산출물 완성), 구현 착수는 C-1~C-12 승인 및 하위 Work ID 발급 후. **(2026-11-27 현재)** C-1~C-12 승인 갈음, P00~P08 구현 완료, P09(문서 정리)만 잔여. |
