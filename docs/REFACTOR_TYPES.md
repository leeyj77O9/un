# TD-18 통합 설계: 다섯 개의 "타입" 표현

상태: **Proposed** (2026-11-19)
관련: `docs/TECHNICAL_DEBT.md` TD-18, `docs/REFACTOR_LOG.md`(Pass 23), `docs/DECISIONS.md`
D-001~D-079

## 0. 요약

Un은 지금 "타입"을 **다섯 개의 병렬 표현**(and 각각을 잇는 세 개의 문자열 어휘)로 관리한다.
어느 하나도 독립적으로 제거할 수 없다 — 전부 실제 로직에서 읽힌다 — 하지만
① eager하게 할당되는 dead-field 제거, ② 중복 표현 제거, ③ 문자열 비교를
Rc 포인터 비교로 바꾸는 순서로 진전시킬 수 있다. 이 문서는 다섯 표현을 증거와 함께
열거하고, 무엇을 안전하게 지울 수 있는지 / 어떤 건 그대로 두는 게 맞는지 / 어떤 건
지연·축소 가능한지를 정한다. **아직 구현하지 않는다. 게이트와 순서를 먼저 정한다.**


## 1. 문제 정의

SL: 하나의 의미론(타입)을 다섯 곳이 각각 들고 있는 상태다. 어긋나면 조용히 틀린다.

예: `declared`는 아무 읽기 없이 매 객체마다 `Rc::clone`을 들고 다니고, class
layout은 `jit_bc.rs`의 인덱스 비교와 `world::class_layouts`의 이름 문자열 비교로
**두 번** 구현돼 있다. 이 문서는 그것들을 열거한다.

## 2. 다섯 표현의 실측 목록

### 2.1 `Types` (`rust/src/runtime/types.rs`)

완전한 타입 대수: `Single(Rc<str>)`, `Union(Vec<Types>)`, `Collection(Rc<str>, Box<Types>)`.
`accepts`/`contains`/`display`/`create`/`name`/`single`을 갖는다.
- 사용: `obj_is`(`obj.rs:967` — `o.borrow().types`를 읽어 `types.contains(&other_type)`),
  `obj.rs:356`(`is_any`?), `obj.rs:1079/1088`(복사), `function.rs:605-606`(clone)
- intern: `world::intern_type`으로 Rc<str>를 캐시(`world/caches.rs:19`)
- 비용: `ObjData::new`가 매 `Single`마다 `Types::create`를 부른다 (2.4 참고)

### 2.2 `ObjData`의 세 필드 (`rust/src/runtime/obj.rs:153`)

```rust
pub struct ObjData {
    pub type_name: Rc<str>,   // 이 객체의 구체 타입 (intern됨)
    pub declared: Rc<str>,     // 원래 소스 이름 (intern됨, 남겨 놓은 clone 하나)
    pub types: Types,          // {type_name} ∪ {obj_type}의 create() 결과
    ...
}
```
- `obj_new`: `types = create(&[Single(obj_type), Single(type_name)])` — `obj_type`은
  `world::obj_type()`(클래스 메타타입), `Single`의 type param은 보통 `type_name`과 동일하다.
- `is_none`/`is_bool`/`is_err` 등은 `type_name` 문자열을 직접 비교한다 (`obj.rs:254+`)
- `declared`: `function.rs`의 `clone_obj`에서만 `.clone()` — **semantic reader 없음** (dead)
- `announced`: 없음. `declared`는 복사 전용.

### 2.3 `Payload::TObj(Types)` (`rust/src/runtime/obj.rs`, Payload enum)

클래스 객체를 first-class 값으로 표현. `collections::tobj(Types::Single(name))`이 생성.
`class_tobj(name)`가 world 캐시(`classes_tobj`)로 per-world 하나씩 만들어 재사용.
- `x is Class` 비교에서 읽힘 (`obj_is`, `collections::tobj_is/t sync`)
- `TryAttr`류의 나머지와 달리 **의미론**을 갖는다: 나중에 class hierarchy의 `__is__` /
  `__contains__`와 연결된다

### 2.4 `ObjData::new`의 eager `Types` (2.1의 파생, 별도 문제로 승급)

```rust
types: Types::create(&[Types::Single(obj_type), Types::Single(type_name.clone())]),
```
`Types::create`는 Vec<Types>를 만들어 sort+contains로 단일/Union을 판정한다. 이 계산은
**매 객체 생성마다** 실행된다 — float 리터럴 하나마다(obj_new("float")), 에러
객체 하나마다, dict/list 하나마다. 캐시(pool)되는 int/str/bool은 풀에서 재사용되기
때문에 비용이 보이지 않는다.

### 2.5 문자열 어휘 세 벌

1. **Un builtin 이름** — `REGISTERED_TYPES` (`world/caches.rs:14`): "obj", "int",
   "float", "dict", "func", ..., "none", "any", "T", "U", "skip", "break".
   intern_type 캐시 대상. `Types::Single`의 name으로 쓴다.
2. **Interned variants** — `world::intern_type`으로 전부 `Rc<str>`. `intern_type` 호출이
   핫패스에 드물게(풀링 안 된 floats/errors/dates만) 남아 있다.
3. **.NET 스타일 이름** — `"System.ArgumentException"` 등 panic object 이름
   (`members.rs:590,1378`). negative manifest가 substring으로 매칭. 타입 로직에서 쓰지 않음.

### 2.6 JIT의 layout keying

`jit_bc.rs`의 class intrinsic은 layout을 **proto_index → Vec<String>** (인덱스)로 들고,
런타임에는 `world::class_layouts`의 **class name → Vec<String>**을 seed한다. 검사할 때는
`name == field` + `Payload::Int(_)`를 매번 **문자열 비교**로 확인한다
(`un_instance_int_set_name`, `un_instance_int_ok_name`).

## 3. 안전하게 지울 수 있는 것 / 그대로 두어야 하는 것

### 3.1 즉시 지울 수 있는 것 (small wins, 검증 비용 최소)

- **`ObjData::declared` 삭제**. reader가 없다(references는 `function.rs:605`의 clone 뿐).
  생성 경로(`plain`, `int_obj`, `str_obj`, `bool_obj_new`, `obj_new`, `err`)에서
  `declared` 필드를 제거하고 `function.rs`의 copy를 맞춘다. ABI 변화 없음(내부 타입).
- **`poc_name`/`obj_type` 중복**: `payload.is_float()` 등은 이미 payload를 직접 읽는다.
  `type_name`이 사실상 payload의 동의어이므로, 고급 경로는 `payload_ref`처럼 borrow 사단을
  피해다닐 수 있는 helper를 하나 둔다 (`int()`류의 풀링이 이미 그렇게 함).

### 3.2 지연·축소 가능한 것 (perf 레버)

- **`types`를 lazy로**. `o.types.clone()` 호출자는 `obj_is`, `function.rs` copy,
  `obj.rs:356`(이건 어디?)뿐이다. 읽지 않는 객체는 `Types`를 만들지 않는다:
  - 방안 A: `Rc<Types>`를 ObjData에 lazy cell로 — 계산 후 캐시 (feature flags 필요 없음)
  - 방안 B: `type_name` 비교만으로 대체 가능한 읽기 지점은 전부 `type_name`으로 대체
  - 어떤 방안이든 `ObjData::new`의 `Types::create` 호출을 없앤다.
- **`intern_type`에서 `type_cache` borrow를 세 번 하는 것** — 이미 풀 캐시가 해결한다.
- **`type_name` 비교**: `is_none`/`payload Star` 비교류는 `Rc::ptr_eq`로 바꿀 수 있다
  (intern된 문자열이면 ptr eq가 정상 동작 — 검증 필요)

### 3.3 그대로 두어야 하는 것 (의미론 보존)

- **`Types` 자체** — union/collection/any 의미론의 구현체. 제거 불가.
- **`Payload::TObj`** — class 객체를 값으로 다루는 근간. 제거 불가.
- **`.NET 이름`** — negative manifest 계약 (D-024/D-027). 보존.
- **`class_layouts` 이름 키** — seed/lookup을 proto_index 하나로 몰면
  `<module>` scope 경계가 깨진다(runtime은 name만 알고, jit은 proto_index만 앎).
  이름이 두 층을 잇는 다리로 맞는 구조다. 초기자(`set_class_layout`)는 유지.
- **`name == field` / `Payload::Int(_)` name-check** — JIT의 안전 장치. 바이트 비교로
  바로 최적화하지는 않더라도 의미는 유지해야 한다.

## 4. 설계

### 4.1 구조 모델

```
읽기                현재                              제안
type of x           o.borrow().type_name (Rc<str>)    동일 (Rc<str>), 내부는 필요시 ptr-eq 헬퍼
declared type       o.borrow().declared (Rc<str>)      삭제
value type set      o.borrow().types (Types, eager)    지연 계산(Rc<Types>) 또는 type_name 파생으로 대체
class object        Payload::TObj(Types::Single(n))    동일
class layout        class_layouts[name] -> Vec<str>    동일 (이름이 runtime↔jit의 다리)
```

### 4.2 제안 절차 (독립적으로 병합 가능)

1. **Phase 1 — `declared` 제거** (2.2/3.1)
   - `ObjData::declared` 필드 삭제, 생성자/복사만 손댐
   - 검증: build(0/0), differential 88/0, test.ps1, cargo test
2. **Phase 2 — `types` lazy**
   - `types: Types::create(...)` eager → `types_cache: RefCell<Option<Rc<Types>>>`
     (or lazy `Rc<Types>` + `OnceCell`)
   - 읽기는 전부 `ObjData::types()` 메서드로 수렴
   - `obj.clone()` copying 경로 조정 (OnceCell clone 동작 확인)
   - 검증: 위와 동일 + bench로 object-allocation-heavy 경로 비교
     (e.g.`float` 리터럴 루프, 에러 객체 생성 루프 — 제안서에 bench 파일 이름 나열)
3. **Phase 3 — `is_*`류의 문자열 비교를 intern-id 비교로 대체** (optional)
   - `is_none` 등이 쓰는 `type_name` 문자열 == 를 `ptr_eq`로 치환
   - 풀링 안 된 객체에서만 ptr_eq가 실패할 수 있으므로 `Rc::ptr_eq + str compare`
     안전망으로 시작하는 것이 안전 (field logic이 intern 전제라 실패하지 않지만,
     방어적으로 혹은 phase 2 완료 후)
4. **Phase 4 — JIT의 `name == field` 바이트 비교** (optional, 별도 자격)
   - `==` 를 `name.as_bytes() != field.as_bytes()`로 교체 — 이미 되어 있음.
     `as_bytes()` 대신 pointer identity를 쓸 수 있나 검토(없으면 drop)
5. **Phase 5 — lobby**: `REGISTERED_TYPES` 외부의 임의 이름이 `type_cache`에
   들어갔을 때의 동작을 문서화 (이미 그렇게 동작하고 있는지 확인)

### 4.3 검증 기준 (구현 완료의 정의)

- build 0 errors / 0 warnings
- differential.ps1: 88 files, 0 differences
- test.ps1: Positive passed / Negative matched / Known 0 유지
- unbc-roundtrip.ps1 -IncludeNegative: identical
- cargo test --release: 205 passed
- bench: object-allocation 및 attribute-set 벤치가 회귀하지 않는다
  (`class1000`, `class.un` 등 — 숫자 표로)
- UV_JIT=2 strict canary green (변경이 JIT에 영향을 주는 경우)

## 5. 위험과 대책

| 리스크 | 확률 | 대책 |
|---|---|---|
| `types` lazy로 바꾸면서 `is_any`/`obj_is`의 union 의미론이 조용히 깨짐 | 중간 | union 의미론을 검증하는 단위 테스트를 먼저 작성 — TDD gate |
| `declared` 제거가 native .NET interop과 어긋남 | 낮음 | interop은 `declared`를 읽지 않음(검증 완료) — 지우고 differential로 확인 |
| `class_layouts`가 name-keyed인 설계를 깨면 hot layout이 깨진다 | 높음(지울 경우) | 그대로 유지 (3.3) |
| OnceCell lazy가 clone 못 따라감 | 중간 | Phase 2 전에 clone 의미론을 설계 단계에서 결정 |
| `type_cache` borrow 재入 (None) | 낮음 | 차감 불필요 — 기존 함수 시그니처 그대로 이용 |
| 단위 테스트가 부족해서 문서만 생기고 구현이 방치 | 높음 | 각 phase마다 1줄짜리 방향으로 이어지는 TODO를 리포트 §7에 |

## 6. 구현 순서 제안

1. Phase 1+2를 한 커밋으로 (`types` lazy + `declared` 제거) — differential로 검증
2. Phase 3을 별도 커밋 (`ptr_eq` 의의) — 독립적
3. Phase 4/5는 그 이후 검토 (non-blocking)
4. 문서에서 `docs/REFACTOR_LOG.md` 항목 발행, `docs/TECHNICAL_DEBT.md` TD-18 마감 예정

## 7. 리뷰어에게

1. Phase 2(방안 A vs 방안 B)의 선택 — `types`를 lazy Rc<Types>로 둘지,
   호출자를 `type_name` 기반으로 전부 바꿀지.
2. Phase 3을 **진짜로** 할지 — `ptr_eq` 도입이 intern invariant를 깨는지 여부에
   따라 답이 다르다. 그 검증을 Phase 2에서 같이 하려면 순서를 바꿔야 한다.
3. `class_layouts` name keying에 대한 별도 결정이 필요하다고 생각하는지 — 이것도
   TD-51과 이어지는 주제다.
4. Phase 4/5를 계획에 포함할지, 보류로 남길지.
