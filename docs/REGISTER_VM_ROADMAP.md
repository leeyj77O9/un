# Register VM 설계 문서 (Phase 2a/b)

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

### Phase 2b (이번 턴부터, 미완)

- 더 작은 스크래치 파서 — 정수 할당자/함수 호출/return만
- `for i in start..end` 를 vm2 시험 opcode로 번역
- 측정: `nested_sum` 벤치 vm2 vs vm

각 단계는 이전 단계를 깨뜨리지 않고 계속 그 자리를 지켜야 합니다 — 즉, 먼저 Commit 완료 후 다음 단계.

## 남은 과제

- vm2 stack limit / stack check 재사용
- 해석 카운터 (dispatch overhead 측정: 소스가 VM1 vs VM2에서 몇 opcode를 처리하는지)
- `.un` 테스트스위트 vm2와 VM1 비교 harness

## 제약

- 외부 crate 없음(차후 Cranelift 도입 시 정책 재논의)
- 기존 테스트 스위트(1363 어서션)를 깨지 말 것
- 기존 `vm.rs`는 새로 쓸 코드와 직교로 유지 — 활성화 전까지 전원 vm으로 초기화
