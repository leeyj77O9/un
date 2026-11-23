# COMPILED_LANGUAGE.md — Un 컴파일 언어 사용 가이드

> 작성: 2026-11-24, Work `W-2026-11-24-ECO1` (TOOL-08 + 문서 담당). 커밋 없음, 코드 수정 없음.
> 목적: `docs/project/COMPILER_LANGUAGE_GAPS.md` 분석 보고를 사용자 대상 가이드로 정리한다.
>
> **확인 필요**: 분석 보고 원본(`docs/project/COMPILER_LANGUAGE_GAPS.md`)은 이 작업 트리에 존재하지 않는다.
> 대신 `docs/project/COMPILER_LANGUAGE_ROADMAP.md`(3행이 해당 보고를 근거로 인용)과 아래 "부록 A"의
> 내부 문서·구현을 직접 확인해 작성했다. 문서 간 수치가 다르면 실제 구현을 우선하고 `확인 필요`로 표시했다.
>
> Compile Mode 계열 명령(`un check`, `un native`, `un obj`, `un build`)은 `docs/guides/CLI.md:224-227`에서
> **Experimental (in development, may change)** 로 분류되어 있다. "제안" 표시가 없는 항목은 현재 구현/문서 기준이다.

## 0. 범위와 용어

| 실행 경로 | 명령 | 실행 주체 | 진입 의미 |
|---|---|---|---|
| 인터프리터 | `un run <file.un>` | Register VM | 모듈 최상위 문장 실행, `main()`은 명시 호출 시에만 |
| 타입 검사 | `un check <file.un>` | typeck만 | 실행 없음, 어노테이션 필수 |
| 타입 검사 + VM 실행 | `un native <file.un>` | typeck → VM | `un run`과 byte-identical 비교 대상 |
| 오브젝트 | `un obj <file.un\|file.unbc> [-o out.obj]` | Cranelift | `un_main` export, 링크는 사용자/드라이버가 수행 |
| 독립 exe (plain) | `un build <file.un> [-o out.exe]` | 네이티브 실행만 | **`fn main()`만 실행**, VM 없음 |
| 내장 exe | `un build --embed <file.un>` | `.unbc` + 전체 VM | 인터프리터 의미론, 모든 프로그램 지원 |

- 근거: `docs/guides/CLI.md:229-287`, `docs/compile/OBJ_MILESTONES.md:30-38`, `AGENTS.md` 빌드/링크 항목.
- 네이티브 산출물의 핵심 보증은 `un run`과 stdout/stderr/exit code **byte-identical**이며,
  회귀 게이트는 `scripts/exe-diff.ps1`(현재 42/0)이다(`docs/compile/OBJ_MILESTONES.md:71-74`).
- 게이트 수치는 문서마다 차이가 있다(예: `COMPILER_LANGUAGE_ROADMAP.md:23`의 workspace 254 vs `AGENTS.md`의 261).
  정확한 현재값은 실행 시 재측정 — **확인 필요**.

## 1. 공개 ABI 계약

### 1.1 진입점 `un_main`

- `un obj`는 진입 proto(기본 `main`)를 **`un_main`** 으로 export한다. 시그니처는 C ABI `() -> i64`다
  (`rust/src/objify.rs:483`, `docs/compile/OBJ_MILESTONES.md:26`, `docs/guides/CLI.md:253-254`).
- 모든 Un 값은 i64로 하강되므로, 엔트리 포인트가 export하는 결과도 int다. Float 반환을 출력하려면
  드라이버에서 별도 처리가 필요하다(`docs/compile/OBJ_MILESTONES.md:27-28`).
- 생성된 드라이버는 `extern "C" { fn un_main() -> i64; }`를 선언하고, 반환값을
  `std::process::ExitCode::from(code as u8)`로 프로세스 종료 코드에 매핑한다
  (`rust/src/main.rs:1086-1089, 1188-1189`).
- 내부 함수는 `un_proto_<i>` / `un_tir_<i>` 이름의 **Local** 심볼로 남으며, `llvm-nm`으로 확인할 수 있다
  (`docs/compile/OBJ_MILESTONES.md:62-63`). 이 심볼들은 공개 ABI가 아니다.
- lowering 실패 proto는 호출 시 trap하는 **trap stub**으로 정의해 링커가 항상 유효한 심볼을 갖게 한다
  (`docs/compile/OBJ_MILESTONES.md:23-25`).

### 1.2 두 가지 호출 규약 (혼동 금지)

| 구분 | 사용처 | 규약 | 근거 |
|---|---|---|---|
| 내부 엔트리 ABI | Compile Mode/JIT가 `jit_cache`에 등록하는 core 함수 | `out: *mut i64` (16B `value,tag`) + `param_tags` + i64 인자. `bit p`=Int/IntVec, `bit 32+p`=Float bits, `bit 40+p`=Bool. 반환 tag `0=Int,1=Float,3=Bool,4=None,2=Bail` | `docs/compile/COMPILE_DESIGN.md:303-322` |
| 공개 링크 ABI | `un obj`/`un build` 산출물 ↔ 드라이버 ↔ 런타임 | `un_main() -> i64`, `unrt_*`/`un_math_*`/`nrt_*` C 함수, 값은 `u64` 핸들 또는 `i64`/`f64` | `rust/src/objify.rs:483`, `rust/src/unrt.rs`, `rust/nrt/src/lib.rs` |

- 내부 ABI의 tag `2=Bail`은 "진입 가드 불일치 → 인터프리터 재실행" 신호이며 공개 ABI의 에러 Obj와 다르다.
- `jit_cache.rs`는 bytecode JIT 엔트리(`JIT_FNS`)와 TIR 엔트리(`TIR_FNS`)를 **별도 맵**에 보관한다
  (`docs/spec/BYTECODE.md:220-224`). 공개 `un_main`과도 별개다.

### 1.3 `unrt_*` / `un_math_*` C ABI

- VM 측 구현은 `rust/src/unrt.rs`, no-VM 측 구현은 `rust/nrt/`(외부 의존성 0)이며,
  두 구현은 `natcode.rs`의 import 선언과 1:1로 대응한다(`SESSION_ARCHITECTURE.md:323-326`).
- 값 표현: NRT는 `#[repr(C)] NObj { tag: i64, a: i64, b: i64 }`를 `Box::into_raw`로 할당하고
  `u64` 핸들로 주고받는다(`rust/nrt/src/obj.rs:6-12, 39-47`). tag 의미는
  `0=Int, 1=Float, 2=Bool, 3=List, 4=None, 5=Error, 6=Str, 7=Dict, 8=Closure, 9=Class instance, 10=Panic`
  이다(`rust/nrt/src/obj.rs:9, 85-117`).
- 인자/반환 레지스터: Cranelift는 Float을 XMM(`f64`), Int/Bool/객체 핸들을 GPR(`u64`)로 전달한다.
  NRT의 `call_native_typed`는 등록된 `kinds`(0 Int, 1 Float, 2 Bool, 3 handle)와 `ret_kind`로 실제 C
  시그니처를 재구성해 간접 호출한다(`rust/nrt/src/abi.rs:1-10, 75-92`). Bool은 저바이트만 읽는다.
- 오류 전달: `unrt_write_error`/`unrt_is_pending`/`unrt_take_error`가 pending error 슬롯을 사용하고,
  처리되지 않은 오류는 `unrt_dump_uncaught`가 stdout에 출력한 뒤 `exit(1)`한다
  (`rust/src/unrt.rs:16-103`, `docs/compile/OBJ_MILESTONES.md:125-143`).
- 주의: `unrt_len`은 raw i64가 아니라 **Obj 핸들**을 반환한다(`docs/compile/OBJ_MILESTONES.md:243-244`).
- 안정성 규칙: 새 `unrt_*` 심볼은 ARC-00 승인이 필요하고 **기존 심볼 시그니처 변경은 금지**다
  (`docs/project/COMPILER_LANGUAGE_ROADMAP.md:21`).

### 1.4 링크 방법

**plain `un build` (no-VM)**

```text
source → parse/optimize → typeck → TIR → nrt_supported → obj_compile(.obj)
       → 생성된 driver.rs → rustc + libnrt.a  →  exe (약 5 MB, VM 없음)
```

- 근거: `SESSION_ARCHITECTURE.md:73`, `rust/src/main.rs:988-1017, 1083-1189`,
  `docs/guides/CLI.md:262-273`.
- 링크는 rustup self-contained mingw gcc 경유 `rustc`가 수행한다. `--target x86_64-pc-windows-gnu`,
  `--edition 2021`, `linker=<gcc>`, `link-arg=<libnrt.a>`, `-lmingw32 -lmsvcrt -lmingwex -lgcc ...`
  (`rust/src/main.rs:1196-1208`).
- 드라이버는 `nrt_register_source` / `nrt_push_frame` / 클래스·필드·메서드·λ 등록
  (`nrt_register_class`, `nrt_register_field`, `nrt_register_method`, `nrt_freeze_classes`,
  `nrt_register_lambda`, `nrt_register_global`)을 `un_main()` 호출 전에 수행한다
  (`rust/src/main.rs:1089-1187`). traceback byte-identical의 근거다.

**`un build --embed`**

```text
source → compile(.unbc) → driver.rs(include_bytes!) → rustc + libunvm.a → exe (약 17 MB)
extern "C" { fn unvm_run_embedded(bytes: *const u8, len: usize) -> i32; }
```

- 근거: `rust/src/main.rs:866-934`, `docs/guides/CLI.md:283-287`, `SESSION_ARCHITECTURE.md:74`.
- 임의의 Un 프로그램(클래스, 컬렉션, IO, 문자열 보간)이 동작하며 인터프리터 의미론을 유지한다.
- `.obj` 산출물은 Windows/GNU에서 **COFF**다(`docs/guides/CLI.md:249-251`). 다른 포맷/타깃 지원 여부는
  **확인 필요**.

## 2. 메모리 모델

### 2.1 VM 경로 (`un run`, `un native`, `--embed`)

- 객체는 `pub type Obj = Rc<RefCell<ObjData>>`다(`rust/src/runtime/obj.rs:11`,
  `docs/spec/ARCHITECTURE.md:39-57`). 공유 가변 참조이므로 `borrow()` 중 재진입은 런타임 패닉 대상이다.
- 스칼라는 interned: 정수 캐시 `-1024..100000`, 문자열 풀 ≤10,000개·≤1,000자, bool 단일 객체
  (`docs/spec/ARCHITECTURE.md:59-61`).
- **GC 없음.** `Rc` 참조 카운트 + 풀 + 인터프리터가 기존 전략이며 소유권 기반 관리는 범위 외다
  (`docs/compile/COMPILE_DESIGN.md:344-352`). 순환 참조를 자동 회수하는 메커니즘은 없다.

### 2.2 NRT 경로 (plain no-VM exe)

- 객체는 의도적으로 **누수**된다: `Box::into_raw`로 할당하고 해제 경로가 없다.
  주석은 "프로세스가 짧게 살기 때문에 해제 추적이 필요 없다"고 명시한다
  (`rust/nrt/src/obj.rs:35-47`). 문자열도 `Box::leak`, 리스트/ dict/closure도 `Box::into_raw`다
  (`rust/nrt/src/obj.rs:151-159`, `rust/nrt/src/list.rs:10`, `rust/nrt/src/dict.rs:140`,
  `rust/nrt/src/closure.rs:78`).
- 작은 정수 `0..16384`는 정적 배열 `SMALL_INT_OBJS`로 미리 만들어 hot path에서 할당이 없다
  (`rust/nrt/src/obj.rs:14-33`).
- 클래스 인스턴스 필드는 **bump arena**(`FIELD_ARENA: Vec<u64>` + `FIELD_BUMP`)에 패킹된다.
  reserve/set_len으로 용량을 늘리기만 하고 reset/free가 없다
  (`rust/nrt/src/class.rs:217-242, 281-285`).
- **장기 실행 주의**: plain exe는 실행 중 할당이 단조 증가한다. 서버·데몬·장기 루프 워크로드에는
  부적합하며, 그런 용도에는 `un run`/`un build --embed`(Rc 경로)를 권장한다.
- **확인 필요**: arena reset/해제나 장기 실행 정책은 현재 없고 `docs/project/COMPILER_LANGUAGE_ROADMAP.md:29`
  에서 "메모리 소유권/arena 해제 — 별도 RFC"로 보류되어 있다.

| 항목 | VM (Rc/RefCell) | NRT (plain no-VM) |
|---|---|---|
| 값 표현 | `Rc<RefCell<ObjData>>` | `u64` raw handle → `NObj` |
| 소유/해제 | refcount, 수동 풀 | 누수 + bump arena, 해제 없음 |
| GC | 없음 | 없음 |
| 단기 프로세스 가정 | 아니오 | **예** (`obj.rs:35-36`, `class.rs:217-218`) |

## 3. 호환성 정책

### 3.1 현재 고정된 계약 (정책 아님, 구현 사실)

- `.unbc`의 `Op` enum은 `#[repr(u8)]`로 고정되어 있다. **opcode 값을 바꾸면 기존 `.unbc`가 깨진다**
  (`docs/spec/BYTECODE.md:12-13`, `AGENTS.md`). 예약 영역은 `0x65..=0x7E`다(`BYTECODE.md:226-229`).
- 파일 형식: `MAGIC = "UNB1"`, `VERSION: u16 = 1`, 모든 테이블 length-prefixed·little-endian
  (`docs/spec/BYTECODE.md:163, 172-196`).
- 로드 시 9개 검증(magic/version, opcode/operand, jump 범위, 인덱스 범위, `source_map.len()==code.len()`,
  예외 테이블, 256-register 제한, 패턴 깊이 ≤64, 패턴 tag)을 수행하고 손상 입력은 exit 1로 거부한다
  (`docs/spec/BYTECODE.md:198-215`).
- 호환성 관행: 추가적 pattern tag(5~7)는 VERSION 1을 유지하고, **새 serialization 필드는 proto layout에
  append한 뒤 version을 bump**한다(`docs/spec/BYTECODE.md:211-213, 226-229`).
- 검증 게이트: `unbc-roundtrip.ps1 -IncludeNegative`가 Positive 50/50, compilable Negative 42 identical(9 skip)
  을 확인한다(`docs/spec/BYTECODE.md:216`).

### 3.2 언어/ABI 버전 관리 제안 (아직 정책 없음 — 제안)

현재 저장소에는 언어 버전이나 ABI 버전을 명시적으로 선언·협상하는 정책이 없다(**확인 필요**: 별도 RFC 유무).
다음은 도입 제안이며 구현된 동작이 아니다.

| 계층 | 제안 | 목적 |
|---|---|---|
| 언어 버전 | `un <major.minor>` pragma 또는 CLI `--lang <version>` | 어노테이션 규칙 등 의미론 변경을 파일 단위로 고정 |
| `.unbc` 버전 | 기존 `VERSION: u16`을 유지하되 append-only 필드만 1→2로 bump | `BYTECODE.md:229` 규칙을 공식화 |
| ABI 버전 | `un_main`과 같은 전역에 `un_abi_version()` 심볼 추가, 또는 런타임 매크로 노출 | `unrt_*` 심볼 추가/시그니처 변경 시 드라이버-런타임 불일치 조기 탐지 |
| 호환성 매트릭스 | 지원 언어 버전 × `.unbc` 버전 × ABI 버전 표를 문서화 | 업그레이드 시 사용자 판단 근거 제공 |
| 게이트 | 버전 불일치 fixture를 `test/negative/manifest.txt`에 등록 | `unbc-roundtrip.ps1`/`exe-diff.ps1` 회귀 방지 |

- 제안 원칙: `docs/project/COMPILER_LANGUAGE_ROADMAP.md:20-22`의 "byte-identical", "ABI 신중",
  "`--embed` escape hatch"를 그대로 따른다. 특히 **기존 `unrt_*` 시그니처 변경 금지**는 유지한다.

## 4. 인터프리터 → Compile Mode 마이그레이션

### 4.1 어노테이션 필수

- Compile Mode에서 모든 함수는 매개변수 타입과 반환 타입을 명시해야 한다
  (`docs/compile/COMPILE_DESIGN.md:130-144`).

```un
fn add(a: int, b: int) -> int      # OK
fn add(a, b)                      # compile error: parameter 'a' requires a type annotation
    -> a + b                      # compile error: function 'add' requires a return type annotation
```

- 반환 추론은 가능하지만 초기 버전에서 명시를 기본 정책으로 한다(상호 재귀에서 추론 순서가 결과를 바꾸는
  경로 제거, 시그니처를 계약으로 고정). `-> none`이면 반환값이 없어도 된다(`COMPILE_DESIGN.md:146-152`).
- 규칙은 모듈 단위이며 **인터프리터 모드 동작은 바꾸지 않는다**: 기존 어노테이션 없는 테스트가
  `un run`에서는 통과하고 `un native`/`un check`에서는 거부된다(`COMPILE_DESIGN.md:152-156`).
- 람다(`Lambda` 노드)는 동적 값 `Any`로 취급해 호출 검사를 하지 않는다(`COMPILE_DESIGN.md:157-159`).
- 네이티브 함수는 등록된 시그니처 테이블을 가지며, 미등록 네이티브는 `(Any,...) -> Any` 동적 경계다
  (`COMPILE_DESIGN.md:160-162`).
- 타입 힌트 진단 예: `parameter 'x' of 'f' has non-core type '...' - annotate it with
  int/float/bool/str/list[int]/dict[str,int]/dict[int,int] ...`
  (`rust/src/tir.rs:1225-1261`).

### 4.2 최상위(top-level) 명시적 거부

- plain exe는 **Un `main` 함수만 실행**한다. 모듈 최상위 문장은 실행되지 않고, 종료 코드는 `main()`의
  반환값이다. 진입 로직은 `fn main()` 안에 써야 한다(`docs/guides/CLI.md:275-281`,
  `docs/guides/CLI.ko.md:270`).
- 이에 반해 `un run`/`un native`/`--embed`는 인터프리터 의미론으로 최상위 문장을 한 번 실행한다.
- "조용히 무시"는 제거되어 있다. plain build는 `find_ignored_top_level_statement`로 첫 번째 누락 문장을
  찾아 `top-level statements are not supported in plain build; move them into fn main() or use un build --embed`
  진단을 출력하고 exit 1한다(`rust/src/compiler.rs:48-91`, `rust/src/main.rs:1011-1017`).
- 유지되는 최상위 형태: 선언, import, 단순 전역 초기화(`name = ...`), 관례적 `main()` 호출
  (`rust/src/compiler.rs:93-97`). 이 외의 부수효과는 `--embed`로 이동한다.

### 4.3 `--embed`가 필요한 기능 표

판정 근거는 `rust/src/tir.rs:606-714`의 `nrt_supported`(allowlist)와 `rust/src/main.rs`의 명시 거부다.

| 기능 | plain `un build` | `--embed` 필요 | 근거 |
|---|---|---|---|
| 명시적 `any` / Union 값 | 지원(tagged NObj dispatch) | 불필요 | `tir.rs` `nrt_supported` 승격, `nrt/dynops.rs::unrt_dyn_bin/un` |
| 시간 시드 `random` | 거부(byte-identical 불가) | 예 | `tir.rs:609, 702-706` |
| `bytes` 모듈 / `b"..."`, `fs.read_bytes`/`write_bytes` | 거부(NRT ABI 없음) | 예 | `tir.rs:101-102, 703-706`, `AGENTS.md` |
| 모듈 전역 쓰기 (`has_global_write`) | 거부 | 예 | `tir.rs:602, 712-713` |
| 최상위 부수효과(단순 전역 초기화 초과) | 명시적 진단 | 예 | `compiler.rs:48-91`, `main.rs:1011-1017` |
| 중첩 클래스 선언(제어 흐름 내부) | 명시 거부 | 예 | `main.rs:981-987` |
| member import(`use m { a, b }`), 2차 모듈 import | 명시 거부 | 예 | `main.rs:607-609, 643-648` |
| 기본 생성자 인자를 가진 base class | 명시 거부 | 예 | `main.rs:1027-1036` |
| 타입 없는 computed property / getter | 거부 | 예 | `main.rs:2341`, `tir.rs:3358` 부근, `AGENTS.md` |
| Int/Float/Bool/Str, list/dict, 클래스·상속, 예외, closure, 기본값 인자, 단순 모듈 | 지원 | 불필요 | `tir.rs:606-612`, `OBJ_MILESTONES.md:36` |
| `go`/`wait` (호출되지 않는 함수 포함) | 명시적 진단으로 거부 | 예 | `compiler.rs::find_unsupported_async_node`, `main.rs`, `NATIVE_CONCURRENCY_DESIGN.md` 안 B |
| `flow`/`atomic` 네이티브 호출 | 거부 | 예 | `tir.rs::native_call_ok` |

### 4.4 권장 마이그레이션 절차

1. 모듈의 모든 `fn`에 파라미터/반환 타입을 붙인다(`un check`로 진단 확인).
2. 최상위 실행문을 `fn main()`으로 옮기고, 단순 전역 초기화만 남긴다.
3. `un native <file.un>`으로 `un run`과 stdout/stderr/exit를 비교한다(byte-identical 목표).
4. `un build <file.un>`을 시도하고, 거부 메시지가 나오면 위 표에 따라 코드를 바꾸거나 `--embed`를 쓴다.
5. `scripts/compile-check.ps1`, `scripts/exe-diff.ps1`을 회귀 게이트로 실행한다.
6. 장기 실행 프로세스라면 2장의 NRT 누수 특성을 고려해 `--embed` 또는 VM 실행을 선택한다.

## 5. 도구 로드맵 (Work 단위 제안)

`docs/project/COMPILER_LANGUAGE_ROADMAP.md:25-29`는 패키지 매니저·크로스 컴파일·DAP/디버그 심볼을
"별도 생태계 Work"로 보류하고 있다. 아래는 그 보류 항목을 `W-2026-11-24-...` 형식의 Work로 구체화한
**제안**이며, 승인/착수 상태는 **확인 필요**다.

| 제안 Work | 범위 | 선행/주의 | 완료 조건 제안 |
|---|---|---|---|
| `W-2026-11-24-TOOL1` 증분 빌드/캐시 | `.unbc`·TIR·`.obj` 해시 캐시, `main.rs` build 드라이버 재사용 | `LANG5`(CLI `--opt-level` + 캐시 기초)와 `main.rs` 충돌 → 순차. `LANG3` 병행 주의(`ROADMAP.md:12,16`) | 무변경 재빌드 스킵, 기존 출력 byte-identical |
| `W-2026-11-24-TOOL2` DAP·디버그 심볼 | `un lsp` 확장 또는 DAP 어댑터, `.unbc` source_map(`BYTECODE.md:165-170`) 재사용, 네이티브 심볼/DWARF | Cranelift가 디버그 정보를 내보내는지 **확인 필요**. TOOL-08 소유(`lsp.rs`) | 중단점/스텝/변수 조회, traceback caret과 소스 위치 일치 |
| `W-2026-11-24-TOOL3` 크로스 컴파일 | 타깃별 링커/런타임 선택, WASM playground 경로와 정합 | 현재 Windows/GNU + wasm32만 확인(`main.rs:893-895`, `AGENTS.md`). 다른 타깃 **확인 필요** | 대상 타깃 exe 산출 + `exe-diff` 동치 |
| `W-2026-11-24-TOOL4` 패키지 매니저 | `use` 경로 해석(`Global.PATH`) 위 manifest/버전 고정, 잠금 파일 | 모듈 import 동작(`AGENTS.md`), 3장 버전 제안과 연계 | 선언적 의존성 설치 + 재현 가능한 빌드 |

- 공통 원칙(`COMPILER_LANGUAGE_ROADMAP.md:18-23`): byte-identical 유지, ABI 신중, `--embed` escape hatch,
  기존 게이트 통과. 새 심볼/경계 변경은 ARC-00 승인 사항이다(`SESSION_ARCHITECTURE.md:310`).
- 우선순위 제안: `TOOL1`(빌드 비용) → `TOOL2`(디버깅 경험) → `TOOL3`(배포) → `TOOL4`(생태계).

## 부록 A. 근거 파일 색인

| 주제 | 파일 |
|---|---|
| CLI 계약/실험적 상태 | `docs/guides/CLI.md`, `docs/guides/CLI.ko.md` |
| 설계·호출 규약·메모리 | `docs/compile/COMPILE_DESIGN.md` |
| 오브젝트/링크 마일스톤 | `docs/compile/OBJ_MILESTONES.md`, `docs/compile/COMPILE_PROGRESS.md` |
| 바이트코드/버전 | `docs/spec/BYTECODE.md`, `docs/spec/ARCHITECTURE.md` |
| 객체 모델/ABI | `rust/src/runtime/obj.rs`, `rust/src/unrt.rs`, `rust/nrt/src/obj.rs`, `rust/nrt/src/abi.rs`, `rust/nrt/src/class.rs` |
| 빌드 드라이버/거부 | `rust/src/main.rs`, `rust/src/compiler.rs`, `rust/src/tir.rs` |
| Work/권한 | `docs/project/COMPILER_LANGUAGE_ROADMAP.md`, `SESSION_ARCHITECTURE.md`, `OPENCODE_SESSION_SYSTEM.md` |

## 부록 B. `확인 필요` 항목 모음

1. 분석 보고 원본 `docs/project/COMPILER_LANGUAGE_GAPS.md` 부재 — ROADMAP 인용만 확인됨.
2. `go`/`wait`의 plain build 현재 동작(LANG2 미착수 가능).
3. 언어/ABI 버전 정책의 존재 여부(3.2는 전부 제안).
4. Cranelift 디버그 심볼/DWARF 출력 지원 여부.
5. Windows/GNU·wasm32 외 크로스 컴파일 타깃 지원 여부.
6. NRT arena 해제/장기 실행 정책(별도 RFC 보류).
7. 게이트 정확한 현재 수치(문서 간 254/261, compile-check 33/36 차이).
