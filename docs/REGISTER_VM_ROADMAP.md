# Register VM 설계 문서

## 목표

현재 스택 기반 인터프리터(`vm.rs`)를 **레지스터 기반**으로 재구현하여, 다음 병목을 제거합니다:

- 객체 dispatch overhead: `match Op` + `Rc<Obj>` 생성/소비 비용
- `Scope` HashMap 룩업을 루프 당 1회가 아니라 할당 시 한 번이면 끝내기
- 명시적 슬롯(indices)으로 로컬 변수 접근
- 외부 crate 없이 Rust 단독으로 구현

## 왜 작동하는가

현재 VM은 `Obj`를 매 push/pop마다 Rc/RefCell 대여 + clone을 반복합니다. register VM은:

```
total: i64 = 0
i: i64 = 1
loop:
    regs[3] = Lt(regs[1], regs[2])   # 불필요한 Rc 대여 없음
    if !regs[3]: goto exit
    regs[4] = Mul(regs[1], regs[2])
    regs[0] = Add(regs[0], regs[4])
    regs[2] = Add(regs[2], 1)
    goto loop
```

이렇게 명령 복사와 refcount 증분이 사라지면, 네이티브 Rust에 가까운 실행 속도가 납니다.

## 단계

### Phase 2a (이번 턴 포함, 완료)

- `rust/src/vm2/mod.rs` — skeleton register VM
- Op: LoadConst, Move, Add, Sub, Mul, Lt, Ge, Jump, JumpIfFalse, Return
- i64 타입만 지원
- 인라인 builder로 nested_sum 함수를 컴파일해 실행
- 단위 테스트 4개 (nested_sum small/200)

### Phase 2b (완료)

- `rust/src/vm2/compile.rs` — vm2 바이트코드 emitter를 실제 AST(파서/desugar/optimize 출력)에 직접 매핑.
  정수 서브셋만 지원: 정수/`true`/`false` 리터럴, 지역변수/전역, 산술(`/`·`//` truncating)/비트연산/비교,
  `and`/`or`/삼항, `if`/`elif`/`else`, `while`, `for i in iter.range(a, b[, step])`, `break`/`skip`, `-> expr`,
  함수 정의와 직접 호출, `io.write(x)`/`io.print(x)`(단일 정수 인자).
- scratch builder 대신 실제 파이프라인 사용: `unvm run --vm2 <file.un>` → lex → parse → desugar → optimize → vm2::compile → vm2 실행.
  vm1(`run`)와 동일한 프론트엔드, vm2 백엔드.
- 지원하지 않는 기능은 하드 컴파일 에러(`unsupported operation`) — 절대 조용히 vm1 폴백하지 않음.
- 프레임 = 64 레지스터, locals 0..31(영구), scratch 32..63(문장/표현식 단위 마크-해제 재사용).
  함수 인자는 r0..r{argc-1}, 반환값은 r0. 가변 장수 값(루프 부등호 sgn)은 anon local slot 사용.
- vm2 테스트 6개 통과, 1363 어서션 스위트 통과, vm1과 출력 바이트 일치 검증 완료.

### Phase 3 (완료)

- vm2를 **전체 Un 런타임에 통합**: `vm.rs`는 삭제되고, vm2의 register 실행기가 `vm` 모듈(`rust/src/vm/mod.rs`)이 되었다.
- 접근법: 인증된 vm1 바이트코드를 로드 시점에 1:1 번역(`rust/src/vm/transfer.rs`), register 실행(`rust/src/vm/mod.rs`).
  프론트엔드/검증기는 전면 재사용 — `.unbc` 포맷 그대로 유지.
- 전체 Un 의미론 지원: 문자열/실수/리스트/딕셔너리/JSON/클래스/속성/열거형/인터페이스/매치/`try`/`defer`/`using`/`go`/`wait`/모듈/전역 전부 vm2에서 동작.
  - locals는 슬롯 테이블, 임시값은 ip별 고정 스크래치 레지스터(스택 깊이가 점프 타깃에서 일치함을 validate가 검증)
  - 호출 인자는 프레임별 value/name 버퍼로 수집(전개 포함, 동적 크기 지원)
  - 예외는 `ExcEntry` + vm2 `TryEntry`(레지스터 경계 기록)로 Catch/Finally/Propagate 모두 처리, `Raise`는 흐름 종료로 처리
  - 크로스 모듈 호출은 프로그램 포인터를 키로 thread-local `PLAN_CACHE`에서 plan을 조회
- 코드 규모: vm.rs 약 1400줄 + vm2 약 1600줄 수준이 vm/ 기준으로 통합, ~55KB 단일 실행기.
- 검증: Positive 39파일 / 1363 passed / 0 failed / 0 crashed, Negative 38 matched + 4 divergences,
  `cargo test --workspace` 203 통과, WASM playground 빌드 OK, 전 테스트출력 100% 일치.
- 성능: nested_sum_5000 ≈ 12s (vm1과 동일 수준; 벤치 측정 스크립트 동일하게 동작). 정확한 핫루프 처리량은 Python 수준에는 못 미침(약 0.7M ops/s).

### Phase 4 (완료, Int-특화 Cranelift JIT — 제약 있음)

- 인터프리터 미세 최적화: `obj::int` 캐싱 범위 확대([-1024, 100_000]), primitive operand에 dunder/프로브 생략,
  `ForNext` on `Kind::Range`에서 `UnOp::Next` 우회, `CurSlotScope` 슬롯 소유자 캐시, `FusedIntMulAdd` 정수 fast path.
- **Cranelift 0.137.1 기반 JIT** (`rust/src/jit.rs`, `jit_cache.rs`, `jit_fn.rs`):
  - `vendor/` 아래 패치된 `region 3.0.2`, `cranelift-jit 0.137.1`, `wasmtime-internal-jit-icache-coherence 50.0.1`를 `[patch.crates-io]`로 사용
    (Windows/GNU에서 `dlltool.exe`가 없어 `windows-sys 0.52.0`을 빌드할 수 없기 때문에 raw kernel32 FFI로 교체).
  - `runner::compile_source`가 JIT-eligible 함수를 미리 컴파일해 `jit_cache`(`fn:<name>`)에 등록하고, `vm::call_proto`가 이름으로 조회한다.
  - 지원 범위: i64 파라미터 1개 + i64 지역변수 + `iter.range(a, b)` 루프 + 정수 산술/비교.
    나머지(문자열/객체/Bool/Float/예외/비트연산/`*`-연속 인자 등)는 전부 인터프리터 fallback.
  - `UV_JIT=0` 환경 변수로 런타임에 opt-out. WASM 타깃에서는 JIT를 완전히 비활성화한다(기존 playground ABI 유지).
  - 측정 (개발 머신): `nested_sum_200` 13.4ms → **0.015ms** (~900×), `dict5000` ~8.5 → ~3.9ms,
    `iter10000` ~31 → ~18ms. qsort/rle/math/fib 등 객체-디스패치 경로는 변화 없음.
- **`.unbc` 직접 번역기(`rust/src/jit_bc.rs`) — 진행 중, 기본 비활성**:
  - `vm/transfer.rs`를 그대로 재사용해 Ins 스트림을 Cranelift IR로 옮기는 기계적 번역기. 등록(`plan_for`에서
    `jit_bc::register_eligible`)·이름 아닌 `(program_ptr, proto_index)` 키(`unbc_key`)·N-인자까지 확장된
    `jit_call`·`UV_JIT_BC=1` opt-in까지 end-to-end 인프라는 갖춰져 있다.
  - **현재 결함**: 다중 반복 중첩 루프에서 Cranelift frontend SSA 버그가 의심되는 잘못된 결과를 낸다
    (예: 단순 합계 루프 `f(n) -> total over iter.range(1, n+1)`에서 f(3)=3, f(4..=7)=7, f(8..=15)=15
    같은 마스크 패턴). 단일 반복/첫 반복은 정확하다. 재현·디버깅용 하네스는 제거하고 `UV_JIT_BC=1`일 때만
    번역을 시도하며, 기본값은 인터프리터 fallback이다.
  - 목표: x86-64에서 Cranelift `FunctionBuilder` SSA/Variable 프레임 경계 동작을 정확히 이해(Go에서 `llm`/`brif`
    에서 Variable이 block 파라미터로 승격되는 부분)하고, 그 후 multi-param/if/while까지 확장.

**제약**: 전체 `.unbc` JIT(모든 opcode native 번역)는 아직 완성되지 않았다. 현재 프로덕션 경로는
"i64 부분집합 + 인자 1개"만 native로 옮긴 것으로, 목표(C안)의 첫 번째 단계 성격의 결과물이다.

각 단계는 이전 단계를 깨뜨리지 않고 계속 그 자리를 지켜야 합니다 — 즉, 먼저 Commit 완료 후 다음 단계.

## 남은 과제

- vm2의 객체 할당 최소화(빈번한 `Obj` 할당으로 핫루프가 여전히 Python에 미달)
- Int-특화 fused op 적용 범위를 정수 리터럴 전파까지 확장 (현재는 Int local 누적만)
- 해석 카운터 보강: vm1이 삭제됐으므로 dispatch 오버헤드 비교는 과거 기준으로만 가능
- `.un` 테스트 스위트에 `assert_eq`에 더해 RefCell 카운팅류 디버그 하네스 유지

## 제약

- 외부 crate 기본 없음 — 예외: `vendor/` 아래 패치된 Cranelift 스택(`region 3.0.2`, `cranelift-jit 0.137.1`, `wasmtime-internal-jit-icache-coherence 50.0.1`, raw kernel32 FFI). 기존 코드는 전부 `std`만 쓴다.
- 기존 테스트 스위트(1363 어서션)를 깨지 말 것
- 기존 `vm.rs`는 새로 쓸 코드와 직교로 유지 — 활성화 전까지 전원 vm으로 초기화
