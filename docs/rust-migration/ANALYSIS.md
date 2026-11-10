# ANALYSIS.md — 기존 C# Un 구현 분석

Reference Implementation: `C:\dev\un` (C# / .NET 8, tree-walking interpreter).
이 문서는 Rust 바이트코드 VM이 **정확히 재현해야 하는 동작**을 기록한다.
모든 항목은 실제 소스 코드(`src/…`)를 읽고 작성했다. 추측은 `TODO-VERIFY`로 표시한다.

- 분석 대상 규모: C# 82개 파일 / 약 11,750줄, `test/` 아래 `.un` 스크립트 42개(negative 포함)
- 분석 시점 브랜치: `main` @ `fe05894`
- 분석자: 에이전트(직접 읽은 파일) + 서브 에이전트(Runtime Object Model, Native Modules)

## 0. 전체 파이프라인

```text
.un source
  -> Runner.Load      : 파일 읽기, CRLF/CR -> LF 정규화, Source(path, code)
  -> Lexer            : indentation-aware 토큰 스트림 (Indent/Dedent/NewLine)
  -> Parser           : Node 트리 (NodeKind + Operator + Value + Annotations)
  -> Desugarer        : 복합 대입 -> Binary, if/elif -> IfCase 정규화
  -> Optimizer        : 상수 폴딩/ternary/if/while 단순화, fixed point까지 최대 64 pass
  -> Evaluator        : Context/Scope/Frame 기반 AST 직접 평가
```

핵심 파일 (`src/Interpreter/Runner/Runner.cs:22` `RunCore`):

```csharp
var ast = parsedAst ?? Parse();
var desugarer = new Desugarer();
var desugaredAst = desugarer.Desugar(ast);
var optimization = Optimizer.OptimizeWithStats(desugaredAst);
var evaluator = new Evaluator(Context);
return evaluator.Eval(optimization.Root);
```

`RunCore`는 `finally`에서 `RunDefers()` → `FreeUsings()`를 실행한다.
`PropagateFlow`/`BreakFlow`/`SkipFlow`/`ReturnFlow`가 최상위까지 새면 각각
`propagated error` / `'break' outside loop` / `'skip' outside loop` / `'->' outside function`
오류로 승격된다(`Runner.cs:40-56`).

Rust 이식 시 이 순서는 **Lexer → Parser → Desugarer → Optimizer → Compiler → VM**가 된다.
Optimizer는 AST 단계에 남기고, Compiler는 desugared/optimized AST만 소비한다.
Compiler가 desugar/optimization을 재구현하면 안 된다(동작 차이 원인).

## 1. Source

`src/Interpreter/Runner/Source.cs`

- `Source { Path, Code, Name(확장자 제외 파일명), FullName, IgnoredNewLines:Set<int> }`
- `GetLine(pos)`: `'\n'` 개수 + 1
- `GetColumn(pos)`: `pos - lineStart + 1`
- `GetLineText(pos)`: `IgnoredNewLines`에 등록된 개행을 이어붙여 논리적 한 줄을 만든다
  (들여쓰기 + 공백 제거)
- `Runner.Load`는 개행 정규화(`\r\n`/`\r` → `\n`)를 수행한다. Rust도 같은 정규화를 한다.

CLI 출력 형식(`src/Interpreter/Error/Error.cs` + `src/Program.cs`):

```text
<{FileNameexcludingExtension}>, line [N], column [M]
    <line text>
    <spaces>^^^
{Header}: {Message}
```

- `Header`는 기본 `"error"`, 전파 오류는 `"error"`(`Err.Header`를 사용).
- exit code 1.
- call stack(`Frames`)이 2개 이상이면 `trace:` 블록을 출력한다.
  `stack.Skip(1).Take(10).Reverse()` → `: [line text trimmed] (frame name)`.

## 2. Lexer (`src/Interpreter/Lexer/Lexer.cs`)

### 2.1 토큰 타입 (`TokenType.cs`)

```text
EOF, Error, None, Null,
LParen RParen LBrace RBrace LBrack RBrack,
DoubleQuestion, Dot DotDot Question,
Assign ColonAssign PlusAssign MinusAssign AsteriskAssign DoubleAsteriskAssign
SlashAssign DoubleSlashAssign PercentAssign BAndAssign BOrAssign BXorAssign
LeftShiftAssign RightShiftAssign QuestionAssign DoubleQuestionAssign,
Or And Xor Not, In Is IsNot As,
Equal Unequal LessOrEqual GreaterOrEqual LessThan GreaterThan,
BOr BXor BAnd, LeftShift RightShift,
Plus Minus Positive Negative,
Asterisk DoubleAsterisk Spread DictSpread,
Slash DoubleSlash Percent At,
BNot Bang,
Indexer Slicer Property NullableProperty,
Comma Colon QuestionDot,
Func Class Enum True False If ElIf Else Match For While Break Skip Return
Use Using Comment,
Identifier,
Integer Float String FString Boolean List Dict Set Tuple Iter,
Pair Annotation, Call,
Go Wait, Try Catch Defer,
NewLine Indent Dedent Finally Wildcard Underscore
```

`Positive/Negative/Spread/DictSpread/Indexer/Slicer/Property/NullableProperty/Boolean/List/
Dict/Set/Tuple/Iter/Pair/Annotation/Call/Comment/Wildcard`는 Lex 결과로 **생성되지 않는다**
(미사용 토큰). Rust에서는 최소한으로 줄인다.

### 2.2 들여쓰기 처리

- 라인 시작마다 공백(`' '`=1)과 탭(`'\t'`=4)을 세어 `indent`를 만든다.
- `Peek('\n')`이면 `NewLine` 1개를 내고 라인 시작 상태를 유지한다(빈 줄).
- `indent > current` → `indents.Push(indent)`, `Indent` 토큰.
- `indent < current` → Pop하면서 `Dedent` 토큰을 개수만큼 낸다.
- 최종 `indent != indents.Peek()` → `Error("invalid indentation")` → `throw new Error(...)`.
- 파일 끝에서 `indents.Count > 1`일 때까지 `Dedent`를 내고 `EOF`를 넣는다.
- `indents`는 `[0]`으로 초기화된다.

### 2.3 개행과 그룹 깊이

- `groupDepth`는 `(` `[` `{`에서 +1, `)` `]` `}`에서 -1.
- `groupDepth > 0`이면 `\n`을 `source.IgnoredNewLines.Add(index-1)`에 기록하고 토큰을 내지 않는다.
- `\n` 후 다음 비공백 문자가 `.`이면 개행을 무시한다(점으로 시작하는 연속 표현).
- 그 외 `\n`은 `NewLine` 토큰.

### 2.4 주석

`#`을 만나면 `\n` 직전까지 건너뛴다. 개행 자체는 다음 반복에서 처리된다.

### 2.5 문자열

- `'` 또는 `"`로 시작. 연속 두 개면 **빈 문자열**(`""`, `''`)로 즉시 토큰화.
  세 개면 `triple = true`.
- 이스케이프 `\\`는 문자 2개를 소비(무조건). 내용 자체는 파서에서 unescape한다.
- non-triple은 같은 따옴표로 종료, triple은 따옴표 3개로 종료.
- 미종료 시 `expected closing "` / `expected closing """` / `expected closing '` / `expected closing '''`
  (`triple ? new string(quote,3) : quote` 문자열로 생성).

### 2.6 f-string

- `` ` ``로 시작, `` ``` ``이면 triple. 이스케이프 `\\`는 2문자 소비.
  `\\`가EOF 직전이면 `unterminated escape sequence`.
- 종료 누락: `expected closing triple backtick` / `expected closing backtick`.

### 2.7 숫자 (`GetNumberToken`)

- `0x`/`0X`(hex), `0b`/`0B`(binary), `0o`/`0O`(octal) 접두 가능. digit 누락 시
  `expected hex digit` / `expected binary digit` / `expected octal digit`.
- 십진: 숫자 + `_` 구분자. `_`는 선행 숫자 없이 시작 불가, 연속 `_`는
  `consecutive underscores are not allowed`(throw → Error로 변환됨).
  `.` 뒤 `_`, `_` 뒤 `.`, `_`로 끝나는 숫자는 각각
  `underscore cannot follow decimal point`, `underscore cannot precede decimal point`,
  `number cannot end with underscore`.
  단 `.`가 `..`(range)와 애매하지 않도록 `Peek('.') && PeekAt(1) != '.'` 조건으로 판정한다.
  소수점 이후 숫자 읽기는 선택(`ReadDigits` 결과는 무시).
  지수 `e`/`E`는 `+`/`-` 선택, 이후 숫자 필수(`expected digit after exponent`).
  마지막에 ASCII letter/digit/`_`가 붙으면 `invalid number format`.
- Float 토큰 판정: 소수점 또는 지수 존재.

### 2.8 식별자/키워드

ASCII letter + `_` 시작, 이후 ASCII letter/digit/`_`.

```text
fn class enum true false if elif else match for while break skip use using
in is as and or xor not go wait try catch finally defer none _
```

`_`는 `Underscore` 토큰. 그 외 `Identifier`.

주의: `null`은 키워드가 아니다. `Null` 토큰 타입이 존재하지만 Lexer는 `null`을 만들지 않는다
(`null`은 `Identifier`가 되고, 전역에 `null`이 없으면 `undefined variable 'null'`가 난다).

### 2.9 오퍼레이터

3-character: `??= <<= >>= //= **=`
2-character: `?? ?. .. += -= *= /= %= &= |= ^= == != <= >= << >> // ** ->`
1-character: `= + - * / % & | ^ ~ ! < > . ? @`
단일 문자: `( ) [ ] { } ,`, `:`(`:=` 아님)

- `->`는 `TokenType.Return`이다(키워드 Return과 같은 타입).
- `;`는 `unknown operator`.
- `groupDepth`는 `(` `[` `{` `)` `]` `}`에서만 변한다.

## 3. Parser (`src/Interpreter/Parser/Parser.cs`)

### 3.1 문장 파싱 순서

`ParseStatement()`는 선행 `@annotation`을 먼저 모은다.

```text
get/set 접근기 시작(`get`/`set` + identifier 2개 연속) -> ParseAccessor
Identifier | LParen | Underscore -> ParseAssignment
use -> ParseUse
using -> ParseUsing (내부는 Assign이어야 함)
class -> ParseClass
enum -> ParseEnum
fn -> ParseFunction
if -> ParseIf
for -> ParseFor
while -> ParseWhile
try -> ParseTry
defer -> ParseDefer
break/skip/return -> ParseBreak/ParseSkip/ParseReturn
그 외 -> ParseExpression
```

`annotation`이 붙을 수 있는 문장은 `class`, `enum`, `fn`뿐이다.
다른 위치에서 annotation을 붙이면 `annotations are not allowed here`.

문장 종료 조건: `NewLine | Dedent | EOF`, 또는 `IsStatementStart`(Identifier, Use, Using, Class,
Enum, Func, If, For, While, Try, Defer, Break, Skip, Wait, At, Return).
그 외 토큰은 `unexpected token {Type}`.

`IsAssignmentStart()`가 `Identifier/LParen/Underscore`에서 참이므로 문장 단위 대입은
`ParseAssignment()`(패턴 + `ParseComma`)로 파싱된다.

### 3.2 표현식 계층

```text
ParseExpression       = ParseExpressionAssign        ( := 만 최후방 처리)
ParseExpressionAssign : ParseTernary [ ':=' ParseExpressionAssign ]
ParseTernary          : ParseBinary ['?' ternary-true ':' ternary-false]
ParseComma            : ParseExpression (',' ParseExpression)*   -> Tuple (1개면 그대로)
ParseBinary(p)        : ParseUnary (op ParseBinary(op_prec))*
ParseUnary            : (not|+|-|~|wait|go) ParseUnary | ParsePostfix
ParsePostfix          : ParsePrimary ( call | .ident | fstring-tag | ?.ident | [..] | ? )*
```

- 삼항 연산자 판정(`IsTernaryQuestion`): `?` 다음이 `:`이면 아니요, `?` 다음이 표현식
  시작이 아니면 아니요. 이후 `()`/`[]`/`{}` 깊이를 세며 depth 0에서 `:`를 찾으면 삼항.
- `ParsePostfix`는 `?`가 삼항이면 **break**한다(후위 `?`로 소비하지 않는다).
- 후위 `?`는 `Unary(Question, expr)`로 표현된다.

### 3.3 이항 연산자 우선순위 (`GetBinaryPrecedence`)

```text
 1: or, ??
 2: xor
 3: and
 4: ==, !=
 5: <, <=, >, >=, is, is not, in
 6: ..
 7: |
 8: ^
 9: &
10: <<, >>
11: +, -
12: *, /, //, %
13: **
```

- `is not`은 `Is` + `Not`을 합쳐 `IsNot` 토큰으로 만든다(우선순위는 `IsNot`의 값을 쓴다).
- 이항은 `precedence <= parentPrecedence`이면 중단하는 좌결합 방식이며 우변은
  `ParseBinary(precedence)`로 재귀한다(우결합).

### 3.4 AST 노드

`Node { Start, Length, Kind, Operator, Children[], Value, Annotations[] }`.
`Node.Const(value)`는 `int/long -> Integer`, `double/float -> Float`, `string -> String`,
`bool -> Boolean`, `null -> None`를 만든다.

`NodeKind`:

```text
Identifier Integer Float String FString Boolean None
Tuple List Dict Set
Unary Binary Ternary
Call Property Index Slice
Assign PlusAssign ... DoubleQuestionAssign   (Parser가 Assign + Operator로 통일)
Function Extension Accessor
If ElIf Else Match For While
Return Break Skip
Use Using
Try Catch Finally
Defer
Go
Block Pair
Typed
Class Enum Parameter Argument Alias Annotation
Lambda Target Wildcard Case ClassBases Spread KwSpread IfCase Branch Tagged
```

실질적으로 파서가 만드는 노드: `Identifier, Integer, Float, String, Boolean, FString, None,
Tuple, List, Dict, Set, Unary, Binary, Ternary, Call, Property, Index, Slice, Assign,
Function, Extension, Accessor, If, Branch, ElIf, Else, Match, Case, For, While, Return,
Break, Skip, Use, Using, Try, Catch, Finally, Defer, Class, Enum, Parameter, Typed,
Block, Pair, ClassBases, Spread, KwSpread, Lambda, Wildcard, Tagged, Annotation`.

### 3.5 주요 문법 세부

- **대입**: `ParseAssignment`는 좌변을 `ParsePattern()`으로 읽는다. 대입 연산자
  (`=`, `:=`, `+=`, …)가 없으면 좌변 노드를 그대로 반환한다(`x` 단독 문장 허용).
  연산자 후 우변은 `ParseComma()`(쉼표 목록 → Tuple). 두 번 연속 대입 연산자는
  `assignment chaining is not supported`.
- **패턴**: `ParsePattern` = `ParsePrimaryPattern (, ParsePrimaryPattern)*` → Tuple.
  `ParsePrimaryPattern` = `_` → Wildcard | `ParsePostfix` [`:` Type] → Typed.
  대입 좌변 유효성: Identifier, Property, Index, Slice, Wildcard, Typed(내부), Tuple(전체).
- **타입 annotation**: `ParseType`는 식별자 + `list[int]` 같은 `[T,...]` + `int | str`를
  **원문 문자열 그대로** `Identifier.Value`로 저장한다(`code.Substring`).
  `x: none` / `int | none`은 파싱 오류는 아니지만 type name으로 사용 불가(명세).
- **함수**: `fn name(params) body`, `fn Type.name(params) body`(Extension).
  파라미터: `*args`, `**kwargs`, `name: type`, `name = default`, 단 하나씩.
  파라미터 노드는 `Parameter(value)`이고, `value`는 `Identifier` 또는
  `Unary(Asterisk/DoubleAsterisk, name)` / `Typed(value, type)` / `Assign(value, expr)`.
- **람다**: `fn(params) -> expr`. `Lambda(parameters, Block[Return(expr)])`.
- **클래스**: `class Name`, `class Name(params)`, `class Name: A, B`.
  `Class(name, [parameters], [ClassBases], body)` 파라미터는 필드 목록이 된다(`Stru`).
- **enum**: `enum Name` + indent된 멤버 목록(`Identifier` 또는 `Identifier = value`).
  콤마 개행 허용.
- **match**: `match expr { pattern -> body, ... }`. `->`는 `Return` 토큰이고,
  body는 `ParseTernary()`(삼항까지 허용, 문장 아님). 마지막 콤마는 선택.
- **for**: `for pattern in expr` + block.
- **try**: `try block (catch [ident] block)* (finally block)?`.
- **defer**: `defer block` 또는 `defer expr`(expr은 Block 1개로 감싼다).
- **use**: `use a.b.c {x, y as z} as alias`, `use a.b.c.*`, `use a.b.c *`.
  경로는 `ParseUsePath`가 `.`로 Identifier/Property 체인을 만들고 `.*`는
  `Unary(Wildcard, path)`로 만든다.
  단 `use member` 실패 시 `module '...path...' not found`(Panic)가 난다.
- **f-string**: `` `text{expr}text` ``는 `FString(children...)`가 만든다. children은 문자열
  조각과 표현식. 표현식은 **내부 Lexer/Parser를 새로 돌려** 파싱하고, 토큰 위치를
  부모 오프셋으로 옮긴다. 닫히지 않으면 `unclosed interpolation`.
- **tagged template**: `expr``fstring`` → `Tagged(expr, FString)` (접두 표현식 뒤에
  f-string이 오면 Tagged로 묶인다).
- **인덱스/슬라이스**: `[expr]` → Index(target, expr). `[a:b]`, `[a:b:c]` → Slice.
  `start!`/`end!`/`step!`는 `null`Children이 될 수 있다.
- **인자**: `*expr`(Spread), `**expr`(KwSpread), `ident = expr`(Pair, keyword arg).
- **tuple**: `()` → 빈 Tuple, `(a)` → `a` 단독, `(a, b)` → Tuple, `(a=1, b=2)` → Tuple(Pair).

### 3.6 문자열 리터럴

`RemoveQuotes`는 triple(`"""`/`'''`)을 3+3 잘라내고, 그 외 1+1 잘라낸다.
`Unescape`는 `\n \r \t \\ \" \'`만 처리하고 나머지 이스케이프는 문자를 그대로 남긴다.

## 4. Desugarer (`src/Interpreter/Desugarer/Desugarer.cs`)

- 복합 대입(`+=` 등)을 `Assign(lhs, Binary(op, lhs, rhs))`로 확장한다.
  `:=`은 `Operator`가 `ColonAssign`으로 남고 확장되지 않는다(우변만 평가, 좌변 대입).
- `if`의 `Branch`/`ElIf`를 모두 `IfCase(cond, body)`로 정규화하고 `Else`는 유지한다.
- 그 외 노드는 children만 재귀 처리한다.
- CompoundAssign NodeKind는 파서가 이미 `Assign` + `Operator`로 만들기 때문에
  Desugarer의 NodeKind 목록(`PlusAssign` 등)은 실질적으로 사용되지 않는다.

Rust 설계: `Assign(target, value, op)`로 표현하고, Compiler 단계에서
`op`이 연산이 있는 compound면 `dup target → load rhs → op → assign` 순으로 컴파일한다.
`:=`은 `op`가 없으므로 `value`를 스택에 남긴다.

## 5. Optimizer (`src/Interpreter/Optimizer/Optimizer.cs`)

- fixed point까지 반복, 최대 64 pass. 64패스 내 수렴하지 않으면
  `Panic("optimizer did not reach a fixed point within 64 passes")`.
- `Binary` 양쪽 const면 **and/or/xor 제외** 상수 평가(평가 실패(Panic)는 노드 유지).
  - int/int: `checked` 연산, `/`는 `double` 결과, `//`와 `%`는 0이면 `Panic("division by zero")`,
    `**`는 음수 지수(`Panic("negative exponent for int pow")`)과
    `Panic("integer pow overflow: a ** b")`.
  - int/float, float/int, float/float: `//`은 `(long)(a/b)`, `**`는
    `Panic("float pow overflow: a ** b")`.
  - string + string만 Plus 허용.
  - 그 외 타입 조합은 `Panic("unsupported optimize types: ...")` → catch되어 노드 유지.
- `Ternary`의 cond가 bool const면 해당 branch로 치환.
- `If`의 IfCase cond가 true const면 body로 치환, false const면 case 제거.
  case가 모두 사라지면 빈 Block. Else만 남으면 else body를 그대로 사용.
- `While` cond가 false const면 빈 Block.

Rust 설계: Optimizer는 AST 전 단계에 유지한다. **Optimizer가 const folding 중 Panic를
던지는 경우**(int pow overflow 등)는 C#처럼 "노드 유지"로 처리해야 한다.
Optimizer가 throw한 `Panic`는 런타임 오류와 다른 경로이므로 VM이 같은 메시지를 내지 않는다.

## 6. Evaluator 의미론 (`src/Interpreter/Evaluator/Evaluator.cs`)

### 6.1 스코프 규칙 (가장 중요)

- `Scope`는 `symbols: Dict<string,int>` + `slots: Vec<Obj>` + `parent` chain.
- `Get(key)`는 자기 symbols → parent chain, 없으면 `Err("variable '{key}' not found")`.
- `Set(key, v)`: 자기 symbols에 있으면 slot 갱신. parent에 **key가 있으면** parent로 전파.
  없으면 자기 symbols에 신규 slot.
- `SetLocalOrDeclare`: 자기 scope에만.
- **대입 시 BindName**:
  ```csharp
  if (!localOnly && scope.Get(name) is FieldRef) { field.Value = value; return; }
  if (localOnly || scope.IsFunctionScope) scope.SetLocalOrDeclare(name, value);
  else scope.Set(name, value);
  ```
  → **함수 스코프의 대입은 항상 새 로컬 변수**를 만든다(외부 가림).
  → `for`/`match`/`catch` 같은 블록 스코프의 대입은 parent로 전파되어 루프 누산 변수가 동작한다.
  → `for`는 `loopScope = new Scope(outerScope)`를 만들고 `BindPattern(..., localOnly: true)`로
    루프 변수를 **반드시 루프 스코프에** 만든다.
- 전역 변수는 `Global.TryGetGlobalVariable`로 마지막에 조회된다. 식별자 해석 순서:
  1) scope chain (`FieldRef`면 `.Value` unwrap)
  2) global scope
  3) class name → `TObj(UnType.Create(name))` + annotation 복사
  4) `undefined variable '{name}'`

Rust VM 설계: `LOAD_LOCAL`/`STORE_LOCAL`/`LOAD_GLOBAL`/`STORE_GLOBAL`/`LOAD_UPVALUE`/
`STORE_UPVALUE`가 이 규칙을 정확히 재현해야 한다.
특히 **"함수 스코프 대입 = 선언(shadowing)"**과 **block 스코프 대입 = parent 전파**를
명령어에 스코프 분류 정보로 encoded해야 한다.

### 6.2 대입/패턴

- `EvalAssign`: 우변 평가 → `BindPattern(pattern, value)` → **우변 값을 반환**한다.
  (`x = 5`의 표현식 값은 `5`, `a, b = (1,2)`도 tuple 반환)
- `BindPattern`: Identifier / Wildcard(무시) / Typed(name만 bind) / Tuple(Unpack + 길이 검증)
  / Property(`SetAttr`) / Index(`SetItem`) / Slice(List 전용, `step != 1` 미지원,
  `end < start`이면 `end = start`, `common = min(sliceLen, rhs.Count)`로 overwrite 후
  `Insert`/`RemoveAt`로 길이 조절).
- tuple unpack mismatch: `tuple unpack mismatch`; `Unpack` 실패는 `cannot unpack {TypeName}`.
- Slice assignment 메시지: `slice assignment only supports list`,
  `slice start must be int`, `slice end must be int`, `slice step must be int`,
  `slice assignment with step != 1 not supported`, `slice assignment value must be list`.
- `Assign`의 `NodeKind.Tuple` 좌변은 `AssignTuple`: `expected {n} values, got {m}`.

### 6.3 Unpack

```csharp
Tup  -> t.Value
List -> l.Value (Count 기준으로 trim)
Spreads -> s.Value
그 외 -> obj.Spread().As<Spreads> ? s.Value : [Err("cannot unpack {TypeName}")]
```

### 6.4 Unwrap (에러 전파)

`Unwrap(obj, node)`는 `obj is Err`면 `Error(err.Message, node)`로 **던진다**.
대부분의 연산은 `Unwrap(...)`를 거친다 → 런타임 `Err`가 예외로 승격되는 지점.

`?` 연산자(EvalUnary의 `Question`):
- 내부 평가 중 `Error`가 나면 `Err(msg, header)` 만들고, `Frames.Count > 0`이면
  `PropagateFlow`로 던지고, 최상위면 그대로 던진다.
- 값이 `Err`이면 `Frames.Count > 0` → `PropagateFlow`, 최상위면 `Error(err.Message, ...)` throw.
- 그 외 값은 그대로.

즉 `?`는 "에러를 예외로 승격"한다. `try`가 잡는 단위와 `PropagateFlow`가 만나는지
주의해서 검증해야 하는 부분(테스트 `test/negative/catch_clobbers.un` 등).

### 6.5 이항 연산

- `and`: 좌변 `ToBool`이 false면 **좌변 값을 반환**, true면 우변 평가.
- `or`: 좌변 truthy면 **좌변 반환**, false면 우변 평가.
- `??`: 좌변이 None이면 우변 평가, 아니면 좌변.
- `Is`/`IsNot`: `right is TObj || left is TObj`이면 `right.Is(left)`, 아니면 `left.Is(right)`.
  (`is`는 타입 객체와 값의 관계를 **양쪽**으로 검사한다)
- `In`: `right.In(left)` (즉 `x in list`는 `list.In(x)`).
- `..`: `left.RangeTo(right)` → `__range__` 매직 메소드 우선, 없으면 int/float(정수값) 바운드로
  `Iter.Range`. 그 외 `unsupported operand type(s) for ..: '{Type}' and '{other.Type}'`.
- 모든 이항 결과는 `Unwrap`로 `Err` → `Error` 승격.

### 6.6 프로퍼티/인덱스/슬라이스

- `EvalProperty`: target이 None이고 `?.` 연산자 또는 자식 체인에 `?.`가 있으면 `None` 반환.
  TObj면 `Global.GetClass(t).GetAttr(name)`, 아니면 `target.GetAttr(name)`.
- `EvalIndex`/`EvalSlice`: optional chain이면 None.
- `IsOptionalChain(node)`: Property(`?.`) / Call / Index / Slice가 chain에 포함되면 true.

### 6.7 호출

- `EvalCall`: callable 평가 → optional chain이면 None → `EvalArguments` →
  `PushFrame(new Frame(node의 text, source, start, length))` → `callable.Call(args)` → `Unwrap`.
- `EvalArguments`는 `Tup(values, names)`를 만든다. `names`는 positional이면 `""`,
  keyword면 이름, `**dict`는 `key.As<Str>`의 value(또는 `Repr()`).
  Spread의 결과가 `Err`면 `Error(err.Message, ...)`.
  `**`가 dict가 아니면 `** argument must be dict`.
- Frame 이름은 `GetText(node)` → Call 노드의 **원문 텍스트**(예: `add(3, 4)`).
  trace 출력에 사용된다. Rust VM도 같은 trace 형식을 유지해야 negative 테스트가 통과한다.

### 6.8 함수 / 클로저

```csharp
Fn fn = new LFn(body.Children, context) { Name, Closure = context, Args = Fn.GetArgs(parameters, context) };
EvalAnnotations(fn, node.Annotations) // reverse 순서로 decorator 적용
context.Scope.Set(name, fn)            // bindToScope
```

- `Fn`/`LFn`/`NFn`/`Arg`의 정확한 바인딩 규칙은 6.9에서 다룬다.

### 6.9 함수 호출과 인자 바인딩 (`src/Core/Object/Function/LFn.cs`, `NFn.cs`, `Fn.cs`, `Arg.cs`)

`LFn.Call(args)` 순서 (`LFn.cs:9`):

1. `Closure.CallDepth >= Global.MAXRECURSIONDEPTH(1000)` → `Err("maximum recursion depth exceeded")`
2. `scope = new Scope(Closure.Scope, isFunctionScope: true)` — **함수 스코프는 항상 새 스코프**
3. `Bind(scope, args)` — 실패 시 그 `Err`를 반환
4. `Closure.CallDepth++`
5. `context = new Context(scope, Closure.Source, Closure.Frames)` → `Evaluator(context)`
6. body의 각 노드 평가
7. `catch (PropagateFlow pf) → return pf.Error` — **함수 경계에서 전파가 `Err` 값으로 변환된다**
8. `catch (ReturnFlow rf) → return rf.Value`
9. `finally`에서 defers(_pop LIFO_) → usings(`Exit()`), 그 `finally`에서 `CallDepth--`
10. 정상 종료 시 `None` 반환

`NFn.Call`은 같은 순서이지만 body가 C# `Func<Scope, Obj>`이고 defers/usings가 없다(`NFn.cs:11`).

`Fn.Bind(scope, args)` 순서 (`Fn.cs:17`):

```csharp
scope.SetLocal("self", Self);
scope.SetLocal("super", Super);
if (Property is not null) scope.SetLocal("field", new FieldRef(Self, Property));
if (TryBindSimplePositional(scope, args, out var r)) return r;   // fast path
args = UnpackArgs(args);                                         // Spreads 평평화
// args.Name[i]가 빈 문자열이면 positional, 아니면 keyword Map에 TryAdd
//   실패 → Err("multiple values for argument '{name}'")
// Args 순회: name이 self/super면 skip
//   IsPositional(*args) → starArg, keywordOnly = true
//   IsKeyword(**kw)     → kwArg
//   값: positional 소비 → keyword[name] → !IsEssential면 DefaultValue
//        → Err("missing required argument '{name}'")
//   각 값은 scope.SetLocal
// starArg 있으면 rest = positional[posIndex..] → new Tup(rest)
// starArg 없고 positional 남음 → Err("too many positional arguments")
// kwArg 있으면 남은 keyword를 Dict로 → SetLocal
// kwArg 없고 keyword 남음 → Err("unexpected keyword argument '{firstKey}'")
```

- `TryBindSimplePositional`는 `args`에 keyword가 없고 `Spreads`가 없고 `Args`에 `*args`/`**kw`가
  없을 때만 동작한다. 이 경로에서도 누락 인자(`missing required argument`)와
  초과 인자(`too many positional arguments`)를 같은 메시지로 낸다.
- `Arg`는 `Name/Type/IsEssential/IsOptional/IsPositional/IsKeyword/DefaultValue`.
  `Fn.GetArgs(parameters, context)`는 `Parameter` 노드를
  `Identifier`(필수) / `Typed`(이름 + 타입) / `Assign`(optional + default) /
  `Unary(* / **)`(positional/keyword)로 해석한다. default 값은 정의 시점에 평가된다.
- `self`/`super`는 `GetAttr`에서 함수에 붙는다(`Obj.GetAttrFrom`의 `Found:` 라벨에서
  `value is Fn`이면 `value.Self = receiver`, `value.Super = receiver.Super`).
  즉 **메소드 바운드 self는 속성 조회 시점에 점프**, Rust VM도 같은 시점에 `self`를 심어야 한다.
- `Property` getter/setter는 `Args = [Arg("value") {IsEssential}]`(setter만)이고
  `field` 슬롯에 `FieldRef(Self, Property)`가 들어간다. `FieldRef.Value`는
  `property.Read(owner)` / `Write(owner, value)` → backing key `"$" + name`에 저장.

### 6.10 클래스 (`EvalClass`, `Evaluator.cs:693`)

- `name` → `parameters`(Tuple이면) → `ClassBases` → `body` 순으로 자식을 읽는다.
- `parameters`가 있으면 `Stru(UnType.Create(className), fields)`, 없으면 `Obj(UnType.Create(className))`.
  파라미터 이름은 `Identifier` 또는 `Typed`의 자식 이름만 허용, 그 외 `invalid initialize`(원문 `invalid initialize`).
- 어노테이션을 적용한 뒤 `Global.SetClass(className, type)` — **선언 시점에 전역 클래스 레지스트리에 등록**
- bases: 첫 base는 `type.Super = baseType`, `type.Parents = [baseType]`,
  `type.Types = UnionType.Create(type.Types, baseType.Types)`.
  이후 base는 `Parents`에 추가하고 `Types`만 합침 (`Super`는 첫 개만).
  미등록 base → `unknown base class '{name}'`.
- body 순회 1차: `Accessor` 노드를 `accessors[name] = (Getter, Setter)`로 수집.
  duplicate getter → `duplicate getter for property '{name}'`, duplicate setter →
  `duplicate setter for property '{name}'`.
- body 순회 2차:
  - `Function` 멤버 → `EvalFunction(member, bindToScope: false)` → `type.Members[name] = fn`.
    이미 accessor에 있는 이름이면 `property '{name}' conflicts with a method`.
  - `Assign` 멤버 → 우변 평가 후 accessor 이름이면 `defaults[name] = value`,
    아니면 `type.Members[name] = value`.
- accessor 순서(`accessorOrder`)대로 `Property` 객체 생성:
  `Default`는 `defaults[name]`, 없으면 super의 `Property.Default`.
  `Getter`/`Setter`는 `CreateAccessor(node, "get {name}" / "set {name}", property)`.
  `type.Members[accessorName] = property`.

### 6.11 enum (`EvalEnum`)

- member가 `Identifier`면 `constants[name] = Int(i++)`, `Assign`이면
  `constants[name] = Int(value)` 후 `i = value + 1`. 그 외 `invalid enum`.
- `Enu(UnType.Create(enumName), 0) { Members = constants }` → `Global.SetClass(enumName, enu)`.

### 6.12 match

- `value`를 평가한 뒤 각 `Case`마다 **새 `Scope(context.Scope)`**를 만들고 `MatchPattern`를 시도한다.
  성공하면 `context.Scope = scope`로 **전환**하고 body를 평가한다(전환된 스코프는 이후에도 남는다).
- `MatchPattern`:
  - `Wildcard` → true
  - `Identifier` → scope.Set(name, value), true
  - `Typed` → scope.Set(name, value), true (타입 검사 없음)
  - `Tuple` → `Unpack(value)`, `Err`면 throw, 길이 불일치면 false, 재귀
  - `Integer/Float/String/Boolean/None` → `Eval(pattern).Eq(value)` 비교
  - 그 외 `invalid pattern {Kind}`
- 전 case 실패 → `no match case`.

### 6.13 for / while / if

- `for`: `Eval(iterable).Iter()` 실패 시 `'{iterable 원문}' is not iterable`.
  `loopScope = new Scope(outerScope)`를 **루프 전체에 하나** 만든다.
  매 반복 `BindPattern(target, item, localOnly: true)`, body 문장을 순회 평가.
  `SkipFlow` → continue, `BreakFlow` → break. `finally`에서 `context.Scope = outerScope`.
  반환값은 `None`.
- `while`: 조건은 `As<Bool>` (strict bool, truthy 아님) → false면 break. body의 `Skip`/`Break` 처리.
  반환값 `None`.
- `if`: `IfCase`/`ElIf`(desugar 후)를 순회, 조건은 `ToBool()`(truthy) 평가.
  `Else`는 무조건 실행. 조건 통과 시 body 값을 반환. 아무도 통과 못 하면 `None`.
  **`if`는 스코프를 만들지 않는다.**

### 6.14 try / catch / finally / defer / using

- `EvalTry`: `try` block → `catch (Error e)`:
  - 각 catchNode: `Children.Count == 2`면 `varName`, block. catch마다 `new Scope(context.Scope)`,
    `varName`에 `new Err(e.Message, e.Header)` 바인딩.
  - catch body가 정상 종료하면 `caught = true`, `hasError = false`, break.
  - catch body가 `Error`를 던지면 `hasError = true` 후 재throw (catch 스코프는 `finally`에서 복원).
  - 아무 catch도 잡지 못하면 재throw.
- `finally`가 있으면: try/catch 종료 후 **항상** finally block 실행.
  finally가 `Error`를 던지면 그 오류가 우선.
  finally가 성공하고 `hasError && pendingError != null`이면 `throw pendingError`
  → try/catch의 원시 오류가 finally 뒤에 재던져진다.
- `PropagateFlow`는 `catch (Error)`에 잡히지 **않는다**(`Flows`는 `Exception` 하위이며 `Error`의 하위가 아니다).
- `defer`: `context.Defers.Push(body)`, 반환 `None`. 실행은 **컨텍스트 종료 시 LIFO**
  (`Runner.RunDefers`, `LFn.Call`의 `finally`).
- `using`: `EvalAssign(child)` → `Unwrap(result.Entry(), node)` → `context.Usings.Push(result)`.
  `finally`에서 `Exit()`를 LIFO로 실행. `__entry__`/`__exit__` 매직 메소드 필요,
  없으면 `unsupported operand type(s) for using entry: '{Type}'`.

### 6.15 go / wait

```csharp
private Future EvalGo(Node node) =>
    new(Task.Run(() => { var eval = new Evaluator(context); return eval.Eval(node.Children[0]); }));

private Obj EvalWait(Node node) {
    var value = Eval(node.Children[0]);
    if (value is not Future future) throw new Error("'wait' requires Future", node, context.Source);
    return future.Wait();   // Task.Result = 블로킹 대기
}
```

- `go`는 **같은 `context`**를 새 `Evaluator`에 넣어 다른 스레드에서 실행한다.
  즉 C#는 스레드 간 **스코프/객체 공유** 의미이고, `Scope`/`Obj`에는 락이 없다(경쟁 조건 가능).
  `test/negative/go_loop_var.un`은 exit 1을 기대한다.
- `time.sleep` 같은 블로킹 네이티브 호출을 `go` 안에서 `wait` 없이 쓰면 C#는 병렬 실행된다.
  Rust의 single-threaded coroutine 설계(Phase 16)는 이 점을 명시적으로 보고해야 한다.

### 6.16 f-string / tagged template / annotation

- `EvalFString`: `String` 자식은 원문 문자열, 그 외 자식은 `Eval(child).ToStr()` →
  `Str` 아니면 `f-string expression must be a string`. 문자열 concat 결과로 `Str.From`.
  **f-string은 escape를 처리하지 않는다**(`\n`은 문자 그대로; Lexer가 `\\`를 skip하는 것은
  중괄호 escapes만 의미한다).
- `Tagged`(`expr\`...\``): `BuildTemplate`는 `String` 자식을 strings, 그 외를 values로 모은다.
  tag가 `Fn`이 아니면 `tagged template target must be a function`.
  호출은 `fn.Call(Tup([List(values), List(strings.map(Str.From))]))` — **인자 2개: values list, strings list**.
- `EvalAnnotations(obj, annotations)`: **reverse 순서**로 decorator 평가.
  decorator가 `Fn`이 아니면 `annotation must evaluate to function`. `fn.Call([obj])` 결과를 새 obj로 교체.
  `EvalFunction`/`EvalExtension`은 어노테이션 결과가 `Fn`이 아니면 `Err("annotation error")`.

### 6.17 호출 트레이스

- `EvalCall`는 `PushFrame(new Frame(GetText(node), source, node.Start, node.Length))`를 하고
  `finally`에서 `PopFrame()`한다. `Frame.name`은 **call 노드의 원문 텍스트**(예: `add(3, 4)`).
- `Program.cs`는 `Frames.Count >= 2`이면 `stack.Skip(1).Take(10).Reverse()`를
  `trace:` 블록으로 출력한다. Rust CLI도 같은 형식·순서·절취(`Take(10)`)를 유지해야
  `test/negative/`의 stderr 매칭이 통과한다.

## 7. 객체 모델 (`src/Core/Object/`)

### 7.1 `Obj` 계층

```text
Obj(UnType type)
  Self, Super, Parents, Members(Attributes = Dict<string,Obj>), Annotations
  Type (UnType), Types (BaseType, 기본값 UnionType.Create(UnType.Obj, declaredType))
  BaseChain(): Super → Parents[1..]를 seen(set)으로 순회, queue 반환
```

- 연산은 전부 **매직 메소드 dispatch + super fallback + fallback `Err`**:
  `Binary(method, other, superCall, fallback)`는 `TryMethod("__add__", [other])` 성공 시 그 값,
  실패하고 `ValidSuper()`면 `superCall(Super)`(super 객체의 같은 메서드 재호출),
  아니면 `Err("unsupported operand type(s) for +: '{Type}' and '{other.Type}'")`.
- 단 `Eq`는 super fallback 후 `None` 비교 특례(`None == None` → true, 한쪽만 None → false)를 가진다.
  `NEq`는 `__neq__` 없으면 `Eq` 결과 negate, `Bool`이 아니면 `Err("operands must be booleans")`.
  `Gt`/`LtOrEq`/`GtOrEq`는 `__lt__`/`__eq__` 조합으로 fallthrough computed된다.
  `Xor`/`Not`는 `ToBool()`이 `Bool`이 아니면 각각
  `left/right operand must be a boolean` / `operand must be a boolean`.
- `Call` 기본: `__call__` → super → `Err("unsupported operand type(s) for (): '{Type}'")`.
- `GetItem`: `__getitem__` → super → `Err("unsupported operand type(s) for []: '{Type}'")`.
  `SetItem`: `__setitem__` → super → `Err("unsupported operand type(s) for [] = 'value': '{Type}'")`.
- `Slice`: `__slice__(start,end,step)` → super → `Err("unsupported slice for '{Type}'")`.
- `Is`: `__is__` → `Types`가 `UnionType`이면 `Contains(obj.Type)`,
  `UnType`이면 `== obj.Type` → 없으면 super 재귀 → `Bool.False`.
- `In`: `__in__` → super → `Err("unsupported operand type(s) for in: '{Type}'")`.
- `Repr`: `__repr__` → super Repr가 `Super.Type.Name`이면 `Type.Name`, 아니면 super Repr,
  없으면 `Type.Name`.
- `Clone`: `__clone__` → super에 `__clone__` 있으면 super.Clone, 없으면 새 `Obj(Type)`
  (`Members.New()`, `Annotations`, `Self`, `Super` clone, `Parents`).
- `Copy` 기본은 `this` (alias).

### 7.2 속성 조회 순서 (`GetAttrFrom(name, receiver)`)

```text
1. Members[name]가 Property → property.Get(receiver)
2. __getattr__(name) 성공 → Found
3. Members[name] → Found
4. ValidSuper() && Super.Has(name) → Super.GetAttrFrom(name, receiver) → Found
5. Global.TryGetOriginalValue(Type.Name, name) → Found   // 네이티브 타입의 네이티브 멤버
6. Parents[1..]에서 Has(name) 있으면 GetAttrFrom → Found
7. Err("'{Type}' object has no attribute '{name}'")
Found 시 value가 Fn이면 value.Self = receiver, value.Super = receiver.Super
```

`SetAttrFrom(name, value, receiver)`:

```text
1. Members[name]가 Property → property.Set(receiver, value)
2. __setattr__(name, value) 성공 → value
3. Members.ContainsKey(name) → Members[name] = value
4. ValidSuper() → Super.SetAttrFrom(name, value, receiver)
5. Type.Name이 int/float/str/bool/date/none/null → Err("'{Type}' object has no attribute '{name}'")
6. Members[name] = value   // 사용자 클래스는 새로운 멤버로 추가
```

### 7.3 None / Null / TObj

- `Obj.None`(UnType.None), `Obj.Null`(UnType.Null)는 정적 싱글턴. `IsNone()`는 `Type == UnType.None`?
  (`Obj.None`와 `TObj`의 `null` 등 경계 확인 필요 — `TODO-VERIFY`).
- `TObj(UnType.Create(name))`는 클래스 이름이 식별자로 조회되지 않을 때 생성되는 타입 객체.
  `EvalIdentifier`의 3단계. `is` 연산은 TObj가 등장하면 방향을 바꾼다(6.5).
- 클래스 타입 객체는 `Global.GetClass(name)`로 얻고, 어노테이션을 복사해 붙인다.

### 7.4 primitive / collection 상세 (Rust에서 그대로 재현해야 하는 것)

- `Int`: -5..256 interns → `is` identity 관측 가능. `int()`는 `Str` parse 등.
- `Str`: intern pool 최대 10000 entry / 1000 chars (만료 시 전체 clear). `ToBool()`은
  non-empty non-bool 문자열 → `True`. iter는 1-char `Str`. `GetItem` 음수 index 정규화,
  범위 초과 시 `list index out of range` (Str 전용 메시지가 아니라 **list 메시지를 재사용**).
- `Float.Add`는 `Str`과 concat 허용; `-` `*` `/`는 `Str` 거부.
- `List`: `Count`가 backing array 길이보다 클 수 있다(성장 `len*2+1`).
  `Copy()`는 alias, `Clone()`은 deep. `In`은 **subset/overlap semantics** (`x in list`는
  list에 x가 있다는 뜻이 아니라 부분집합/교차 판정). 음수 index는 `+ Count`.
  `sort`/`CompareTo`는 비교 불가 타입끼리 `types that are not comparable to each other.` panic.
  native `map`은 `(value, index)`를 positional로 호출.
- `Dict`: 키는 `Obj`이며 `Hash()` dunder로 해시. `Copy()` alias, `Clone()` shallow.
  `In(dict)`는 key+value pair subset 판정.
- `Set`: `In` 오버라이드 없음 → `unsupported operand type(s) for in: 'set'`.
- `Tup`: named members를 거울로 유지, `ToStr`/slice가 이름을 보존.
- `Date.Add(Date)`는 일수 합. `Json`은 `none`을 `"null"`로 문자열화.
- `Spreads`의 타입 이름은 `spread`.
- 네이티브 메시지의 오타(`invaild`, `invalid argumnets: index`)는 **그대로 유지**해야
  테스트가 통과한다.

## 8. 타입 시스템 (`src/Core/Object/Type/`)

- `UnType`는 이름으로 intern되고, **동등성은 참조 identity**다.
  `CollectionType { Kind[ItemType] }`, `UnionType`는 `sorted a|b` 키로 intern,
  `Any`가 흡수(union에 Any가 있으면 Any).
- `Types`는 `UnionType.Create(obj, declaredType)`로 시작하고 상속 시 합쳐진다.
- Rust: `TypeTable`에서 intern된 `TypeId`를 사용하고, `==`는 id 비교로 재현한다.

## 9. 전역 / 모듈 시스템 (`src/Core/Global.cs`)

- `Global.PATH`(CLI는 현재 작업 디렉터리), `MAXRECURSIONDEPTH = 1000`,
  `FileSystem`(Physical/Memory), `_allowedNatives`(sandbox 허용 목록, `IsAllowed` 실패는
  `Panic("module '{name}' is not allowed in this environment")`).
- `Init(path)`: reflection으로 `BuiltinModule`(전역 함수), `BuiltinType`(타입 객체 + originalClasses),
  `NativeType`(originalClasses)을 등록하고 `originalBaseTypes`를 클래스 상속 순서로 채운다.
  `scope.Set("__name__", Str("__main__"))`.
- `Include(name, alias, imports)`는 native 모듈. `Import(parts, source, alias, imports)`는 파일 모듈.
  파일 모듈 경로: **현재 파일 기준 상대 경로** → `mod.un`/`*.un` 존재 확인 → 없으면 `Global.PATH` 기준.
- `Load(fullPath)`: `Path.GetFullPath` 캐시 키. 캐시 히트는 `map.New()`(얕은 복사).
  `_loading`에 이미 있으면 `Panic("circular import detected: '{fullPath}'")`.
  `.un` 파일 → `LoadFile`; 디렉터리면 `mod.un` 우선, 없으면 `*.un` 전부 로드.
  `LoadFile`은 `new Scope(GlobalScope)` 위에서 `Runner.Load(file, inner).Run()` 실행 후
  `inner`의 symbol table/slots를 map으로 복사한다.
- `ImportMap`: `imports`가 있고 alias가 있으면 `Obj(UnType.Module)`에 멤버 복사
  (`*` wildcards 포함, `module '{moduleName}' has no member '{name}'`,
  `'{alias}' already exists in module '{moduleAlias}'`).
  alias 없이 `imports`만 있으면 symbol table에 직접 `scope.Set(alias, value)`.
  `imports`가 없으면 `moduleAlias ?? moduleObjectName` 이름의 모듈 객체를 scope에 넣는다.
- 네이티브 바인딩: `CreateMethod(Type)`는 public static `[Native]` 메서드를 `NFn`으로 만든다.
  인자는 `[ArgInfo]`의 `Name` 또는 파라미터 이름으로 `args[argName]` 조회, `Err`면 `args[p.Name]`로
  재조회, `[Self]`는 `args["self"]`.

## 10. 네이티브 모듈 인벤토리

`src/Core/Native/`의 `[NativeModule]` 모듈 목록과 각 `[Native]` 함수의
`Name/Description/Example/ReturnType/ArgumentTypes/ArgInfo`는 서브 에이전트가 수집 중이며
`docs/rust-migration/NATIVE_INVENTORY.md`에 저장한다 (`TODO-VERIFY` — 수집 완료 후 채움).

## 11. 테스트 baseline (Rust VM이 통과해야 하는 목표)

`scripts/test.ps1`의 **실측 기준값** (2026-11-10 `main`에서 직접 실행해 확인):

- Positive: **33 files / 1265 passed / 0 failed / 0 crashed**
- Negative: **42 checks matched / 0 mismatched** (`test/negative/manifest.txt`,
  `path|expected exit|expected substring`; exit code와 stderr 부분 일치)
- Known: `test/regression/defects.un` (6 assertions) **KNOWN FAIL로 기록**
- `test/test_suite.un` 스모크: `109 passed, 0 failed`
- `test/MANIFEST.txt`는 `group|path` 행 (suite 1, unit 28, integration 4, known 1, negative는 별도 manifest)
- `test/` 아래 `.un` 파일 83개 (fixtures 포함)
- Negative 중 exit가 0이 아닌 것은 25개, exit 0(표준 출력/무해 확인)이 17개.
- stack overflow 기준: `test/negative/depth_limit_crash.un`과
  `test/negative/property_recursion.un`의 기대 exit는 `-1073741571`
  (`0xC00000FD`, C# 재귀 스택 고갈). `test/negative/stack_overflow.un`은 exit 1을 기대한다.
  Rust VM은 같은 exit code를 재현할지 결정해야 한다 (DECISIONS.md 참조).

## 12. 반드시 보존해야 하는 의미론 (요약 체크리스트)

1. 함수 스코프 대입 = 새 로컬 선언(shadowing); block 스코프(for/match/catch) 대입 = parent 전파.
2. `for`는 루프 전체에 스코프 1개, 루프 변수는 `localOnly`.
3. `if`는 스코프를 만들지 않는다.
4. `and`/`or`는 피연산자 값을 반환한다 (bool 아님).
5. `??`는 `IsNone()` 검사.
6. `x in y`는 `y.In(x)`.
7. `is`/`is not`는 TObj 등장 시 방향을 바꾼다. `is not`은 Bool negate.
8. `?`는 `Frames.Count > 0`이면 `PropagateFlow`, 최상위면 `Error`. `try/catch`는
   `PropagateFlow`를 잡지 못하고, 함수 경계(`LFn.Call`)가 `Err` 값으로 바꾼다.
9. `Unwrap`는 모든 `Err`를 `Error`로 승격한다.
10. `EvalAssign`는 우변 값을 반환한다.
11. call frame 이름은 call 노드 원문 텍스트, trace는 `Skip(1).Take(10).Reverse()`.
12. defers/usings는 컨텍스트 종료 시 LIFO, `finally` 순서: defers → usings.
13. 함수 재귀 한도 1000 → `maximum recursion depth exceeded`.
14. Optimizer는 desugar 후 최대 64 pass fixed point; Panic은 노드 유지로 삼는다.
15. 어노테이션은 reverse 순서 적용, decorator는 `Fn`이어야 한다.
16. enum은 선언 시 `Global.SetClass` 등록, int 값 자동 채움(`i++`, `= value` 후 `i = value+1`).
17. 클래스는 선언 시 `Global.SetClass` 등록, 첫 base만 `Super`, 모든 base가 `Parents`/`Types`에 합쳐진다.
18. Property는 backing key `"$"+name`, getter/setter는 `field` FieldRef를 받는다.
19. 메소드 바운드 `self`는 속성 조회(Found) 시점에 `Fn`에 설정된다.
20. 식별자 해석 순서: scope chain → global → class name(TObj) → `undefined variable '{name}'`.
21. 네이티브 오타/문자열(§7.4)과 exit code는 그대로 재현.

## 13. 알려진 결함 (`test/KnownFailures.txt`)

- `test/regression/defects.un` (6 assertions)이 문서화된 동작과 실제 구현이 다른 것을 기록한다.
  Rust VM이 **C# 동작을 기준으로 삼을지 문서 기준을 따라야 하는지**는 DECISIONS.md에서 결정한다
  (원칙: Reference Implementation의 실제 동작을 기준선으로 하고, 결함은 명시적으로 기록).