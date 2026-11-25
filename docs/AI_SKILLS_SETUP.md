# AI Skills Setup (OpenCode v2.4.1)

이 문서는 2026-11-25에 수행한 OpenCode 스킬/플러그인 설치의 **실제 결과**를 기록한다.
모든 명령은 Windows PowerShell에서 실행했고, 설치·검증까지 완료된 항목만 "설치됨"으로
기재했다. Un 저장소의 소스 코드(`rust/`, `bench/`, `test/`, `main.un`)는 수정하지 않았다.

## 0. 환경 및 전제

| 항목 | 값 |
| --- | --- |
| OpenCode | v2.4.1 (CLI: `C:\Users\User\AppData\Local\Programs\ai.opencode.desktop\resources\opencode-cli.exe`, PATH 미등록) |
| Node / npm / npx | v26.13.0 / 11.20.0 (`npm.ps1` 차단 → `npm.cmd`, `npx.cmd` 사용) |
| Git / Python / uv | 2.56.0 / 3.13.17 / 0.13.7 |
| 전역 설정 | `C:\Users\User\.config\opencode\opencode.json` |
| 설정 백업 | `C:\Users\User\.config\opencode\opencode.json.bak`, `C:\Users\User\AppData\Local\Temp\opencode\opencode.json.bak` |
| 제공자/모델 | `strata` / `strata/swift-iq2_xs` — **변경 없음** |
| 전역 스킬 디렉터리 | `C:\Users\User\.agents\skills` (skills CLI 표준), `C:\Users\User\.config\opencode\skills` (비어 있음) |
| 프로젝트 스킬 | `C:\dev\un\.opencode\skills` (`.gitignore`에 따라 로컬 전용, 미커밋) |

전역 `plugins` 배열은 최종적으로 다음 2개다(프로젝트 플러그인 1개는 별도):

```json
"plugins": [
  "superpowers@git+https://github.com/obra/superpowers.git#v7.0.2",
  "opencode-planning-with-files@1.2.0"
]
```

## 1. 설치된 스킬 전체 목록

### 1.1 Superpowers v7.0.2 — 전역 플러그인

- **소스**: https://github.com/obra/superpowers, 태그 `v7.0.2`, 플러그인 캐시 커밋 표기 `22f0b46`
- **설치**: `opencode-cli.exe plugin add superpowers@git+https://github.com/obra/superpowers.git#v7.0.2`
- **위치**: 플러그인 캐시 `C:\Users\User\.cache\opencode\npm\git-superpowers-4f58c7678957\...\node_modules\superpowers\skills`
- **기능**: 작업 전 스킬 강제(`using-superpowers`), 브레인스토밍, 계획 작성/실행, TDD, 체계적 디버깅, 서브에이전트 파이프라인, 코드 리뷰 송수신, 작업 트리, 검증, 스킬 작성 등 15개:
  `brainstorming`, `diagnosing-superpowers`, `dispatching-parallel-agents`, `executing-plans`, `finishing-a-development-branch`, `receiving-code-review`, `requesting-code-review`, `subagent-driven-development`, `systematic-debugging`, `test-driven-development`, `using-git-worktrees`, `using-superpowers`, `verification-before-completion`, `writing-plans`, `writing-skills`
- **의존성/플러그인**: 전역 플러그인 자체(별도 npm 설치 없음). OpenCode V2에서는 할 일 도구가 없어 마크다운 계획 파일을 쓰고, `subagent` 도구는 `agent`/`model`/`sessionID`를 지원한다.
- **검증**: `opencode-cli.exe plugin list`에 `superpowers 22f0b46` 표시, 세션에 15개 스킬 로드 확인.

### 1.2 Planning-with-files — 전역 플러그인 + 전역 스킬

- **소스**: 플러그인 `opencode-planning-with-files@1.2.0` (npm), 스킬 https://github.com/OthmanAdi/planning-with-files
- **설치**:
  ```powershell
  opencode-cli.exe plugin add opencode-planning-with-files@1.2.0
  npx.cmd -y skills@latest add OthmanAdi/planning-with-files -g -a opencode -y -s planning-with-files
  ```
- **위치**: 스킬 `C:\Users\User\.agents\skills\planning-with-files`, 플러그인 npm 캐시
- **기능**: `task_plan.md` / `findings.md` / `progress.md` 기반 영속 계획, 컨텍스트/컴팩션 훅, idle gate, 도구 `pwf_init` / `pwf_status` / `pwf_check`. `PLANNING_DISABLED=1`로 훅 비활성화 가능.
- **의존성**: 플러그인 1개 + 스킬 디렉터리. 별도 API 키 없음.
- **검증**: plugin list에 `opencode-planning-with-files 1.2.0`, 세션에 `pwf_*` 도구 및 스킬 확인.

### 1.3 Graphify 0.9.119 — 전역 CLI + 프로젝트 스킬/플러그인

- **소스**: PyPI `graphifyy` 0.9.119 (https://github.com/... 대신 공식 PyPI 배포 사용)
- **설치**:
  ```powershell
  uv tool install --python 3.12 graphifyy
  graphify opencode install --project        # C:\dev\un 에서 실행
  graphify extract . --code-only
  graphify cluster-only . --no-label
  ```
  Python 3.13.17에서는 tree-sitter 빌드가 MSVC를 요구해 실패 → `--python 3.12` 사용.
- **위치/산출물**: 실행 파일 `C:\Users\User\.local\bin\graphify.exe`(PATH 포함), 프로젝트 스킬 `C:\dev\un\.opencode\skills\graphify\SKILL.md` + `references\`, 프로젝트 플러그인 `C:\dev\un\.opencode\plugins\graphify.js`, `C:\dev\un\AGENTS.md`에 `## graphify` 섹션 추가(백업: `C:\Users\User\AppData\Local\Temp\opencode\AGENTS.md.bak`), 지식 그래프 `C:\dev\un\graphify-out\`.
- **기능**: 코드→지식 그래프, 커뮤니티 감지, `graphify query|path|explain|section|update`, HTML/JSON/GRAPH_REPORT 산출, MCP 서버, git 훅(선택).
- **의존성**: `uv tool`로 설치된 Python 3.12 환경 + 프로젝트 플러그인 1개. **git 훅은 설치하지 않음**(선택 사항, `graphify hook install`로 추가 가능).
- **검증**: `graphify --version` → 0.9.119; 그래프 5879 노드 / 18040 엣지 / 214 커뮤니티 생성; `graphify query "How does the register VM dispatch opcodes?"` 및 `graphify explain <node-id>` 정상 출력; `plugin list`에 `graphify-38e661d4 local` 표시. `.gitignore`에 `/graphify-out/` 추가(Generated output).
  - 주의: `--code-only`로 추출해 264개 파일(`.un` 스크립트·문서·설정)은 그래프에 없다. AST 추출 실패 1건(`rust/Cargo.toml`)도 있다.

### 1.4 Matt Pocock skills — 전역 스킬 24개

- **소스**: https://github.com/mattpocock/skills, 커밋 `d65391149dbb5fe76478e76fa8d5122b2aa5a70e`
- **설치**: `npx.cmd -y skills@latest add mattpocock/skills -g -a opencode -y --skill '*'` 후 아래 15개를 `npx.cmd -y skills@latest remove <name> -g -a opencode`로 제거(CLI 제거가 디렉터리를 남겨 최종적으로 `Remove-Item`으로 정리).
- **위치**: `C:\Users\User\.agents\skills\<name>`
- **유지 스킬 24개**: `ask-matt`, `codebase-design`, `domain-modeling`, `grill-me`, `grill-with-docs`, `grilling`, `handoff`, `implement`, `implement-spec`, `improve-codebase-architecture`, `pr`, `prototype`, `research`, `retro`, `setup-matt-pocock-skills`, `teach`, `to-questionnaire`, `to-spec`, `to-tickets`, `triage`, `wait-what`, `wayfinder`, `wizard`, `writing-for-agents`
- **기능**: 기획→명세→티켓→구현→회고까지의 에이전트 워크플로, 그릴링(질문 압박), 리서치, 핸드오프 등.
- **의존성**: skills CLI를 통해 복사된 Markdown 스킬뿐(플러그인 없음).
- **검증**: 디렉터리 24개 확인, 세션에 스킬 로드 확인.

### 1.5 Addy Osmani agent-skills — 전역 스킬 11개

- **소스**: https://github.com/addyosmani/agent-skills, 설치 시 HEAD `e997ac5b9a74eab332d5e9793bb38a38c8cfdd74` (핀 없음)
- **설치**: 필요 스킬만 개별 설치, 예: `npx.cmd -y skills@latest add addyosmani/agent-skills -g -a opencode -y -s code-simplification`
- **위치**: `C:\Users\User\.agents\skills\<name>`
- **유지 스킬 11개**: `code-simplification`, `ci-cd-and-automation`, `constraint-driven-development`, `deprecation-and-migration`, `documentation-and-adrs`, `doubt-driven-development`, `git-workflow-and-versioning`, `incremental-implementation`, `performance-optimization`, `shipping-and-launch`, `source-driven-development`
- **기능**: 단순화, 제약/품질 기준, 점진 구현, 성능, 이전/마이그레이션, 문서/ADR, 배포, 출처 기반 개발, CI/CD 등 엔지니어링 품질 게이트.
- **의존성**: 플러그인 없음. 주의: 스킬별 설치는 저장소 공유 `references/` 체크리스트를 copying하지 않는 문제(addyosmani/agent-skills#361)가 있어, 각 스킬 파일 자체만 설치됐다.
- **검증**: 디렉터리 11개 확인, 세션에 스킬 로드 확인.

### 1.6 ECC (Everything Claude Code) v2.2.3 — 엄선 5개 스킬

- **소스**: https://github.com/affaan-m/ECC, 태그 `v2.2.3`, 커밋 `c05b2d6614f62f6db0047669aa4eefb223d478f9`
- **설치**: 임시 디렉터리에 태그 클론 후 로컬 경로에서 개별 설치(저장소 전체를 옮기지 않음):
  ```powershell
  git clone --depth 1 --branch v2.2.3 https://github.com/affaan-m/ECC.git <temp>\ecc
  npx.cmd -y skills@latest add <temp>\ecc -g -a opencode -y -s rust-patterns   # 외 4개 동일 방식
  ```
  최초 시도한 `affaanhaniffarid/ecc` 저장소는 존재하지 않아 실패했고, npm 공식 문서에 명시된 `affaan-m/ECC`로 정정했다.
- **위치**: `C:\Users\User\.agents\skills\<name>`
- **설치 스킬 5개**: `rust-patterns`(소유권·에러·트레잇·동시성), `rust-testing`(Rust 단위/통합/속성 테스트), `iterative-retrieval`(서브에이전트 컨텍스트 점진 보완), `parallel-execution-optimizer`(의존 그래프·작업트리 기반 병렬화), `benchmark-optimization-loop`(기준선→가설 1개→정확성 게이트→승격).
- **의존성/플러그인**: **ECC 플러그인은 설치하지 않음**(OpenCode용이라 하지만 72 agents/99 commands/hook 번들을 함께 등록하게 됨). 순수 Markdown 스킬 5개만 복사.
- **검증**: 디렉터리 5개 확인, 세션에 5개 스킬 로드 확인(`system-update` 알림).

### 1.7 Un 프로젝트 전용 스킬 4개 — 프로젝트 스코프

- **위치**: `C:\dev\un\.opencode\skills\<id>\SKILL.md` (gitignored → 로컬 전용, 커밋하지 않음)
- **작성 근거**: 실제 저장소 조사(코드·스크립트·문서) 결과만 반영. 템플릿/일반론 복사 없음.

| ID | 소스 | 기능 |
| --- | --- | --- |
| `architecture-audit` | `docs/spec/*`, `docs/project/PROJECT_AUDIT.md`, `TECHNICAL_DEBT.md`, `DECISIONS.md`, `REFACTOR_SUMMARY.md` | 아키텍처·문서 드리프트 감사, S1~S2-build 심각도 분류, 동결 불변식(opcode/태그/캐시키/에러 문자열)과 의도된 잔여 부채 판별, 게이트 배터리 |
| `benchmark-engineering` | `scripts/*.ps1`, `bench/*`, `.github/workflows/build.yml`, `BENCHMARK.md` | 6레인 벤치마크(`bench.ps1`), `-Gate`, 게이트 러너별 바이트/텍스트 비교 기준, 변경 후 검증 절차 |
| `compiler-engineering` | `rust/src/{lexer,parser,optimize,compiler,bytecode,vm}*` | 스테이지별 진입점·데이터 타입, `#[repr(u8)]` opcode 동결 규칙과 예약 구간 `0x65..=0x7E`, `.unbc` `UNB1`/v1 와이어 포맷, optimizer의 desugar/상수접기 범위, 새 문법 추가 시 10단계 편집 체인, slot 할당/declaring-scope 의미 |
| `rust-engineering` | `rust/src/{jit_bc,jit_cache,runtime,native}*`, `rust/nrt/*`, Compile Mode | VM 디스패치/레지스터 계획, JIT 적격성·태그·hidden-pointer·class receiver/name-checked intrinsic, 의미 보존 bail→인터프리터, Compile Mode(`check/native/obj/build`)와 `nrt_supported` 게이트, `unrt_*` C ABI·NRT NObj 태그·트레이스백, strict canary |

- **의존성**: 없음(Ordinary Markdown). graphify 스킬과 함께 쓰면 `graphify query`를 먼저 호출하도록 되어 있다.
- **검증**: 4개 모두 세션 `system-update`로 로드 확인 완료. graphify를 포함해 프로젝트 스킬 디렉터리는 5개.

## 2. 설치하지 않은 것과 이유

| 항목 | 이유 |
| --- | --- |
| Matt Pocock 15개: `tdd`, `code-review`, `diagnosing-bugs`, `setup-ts-deep-modules`, `migrate-to-shoehorn`, `scaffold-exercises`, `setup-pre-commit`, `git-guardrails-claude-code`, `writing-beats`, `writing-fragments`, `writing-shape`, `software-factory`, `claude-handoff`, `chief-of-staff`, `loop-me` | Superpowers의 대응 스킬과 중복, 또는 TypeScript/Node/Claude Code 전용 워크플로·에이전트로 Un 프로젝트와 무관. `planning-with-files`는 PWF 플러그인+스킬로 이미 설치됨. |
| Addy Osmani 15개: `accessibility-engineering`, `api-and-interface-design`, `browser-testing-with-devtools`, `code-review-and-quality`, `context-engineering`, `debugging-and-error-recovery`, `frontend-ui-engineering`, `idea-refine`, `interview-me`, `observability-and-instrumentation`, `planning-and-task-breakdown`, `security-and-hardening`, `spec-driven-development`, `test-driven-development`, `using-agent-skills` | Superpowers/기존 설치분과 중복이거나 웹 프런트엔드·제품 기획 특화로 CLI 인터프리터 프로젝트에 불필요. |
| ECC 323개 나머지 | Claude Code 전용 hook/command/agent 번들(`gateguard`, `delivery-gate`, `ck`, `continuous-learning`, `skill-stocktake` 등), 다른 언어/프레임워크 전용, 마케팅용 `benchmark-methodology`(경쟁사 비교), 외부 스킬을 받아오는 포인터(`repo-scan`), 거래용 `recursive-decision-ledger` 등. 가져오지 않음. |
| ECC OpenCode 플러그인 | 설치 시 `opencode.json`에 대규모 commands/agents/hooks 등록이 필요해 "Claude Code 전용 hook/command/rule을 가져오지 않는다"는 요구와 충돌. 순수 스킬만 취함. |
| Graphify git 훅 | 선택 사항이고 `.git/hooks`를 수정함. 수동 `graphify update .`로 충분해 미설치(`graphify hook install`로 언제든 추가). |
| 기타 레포 전체 복사 | 저장소 통째로 복사하면 Claude 전용 `.claude/commands`, 플러그인 매니페스트까지 들어옴. 필요한 스킬만 골라 설치함. |

## 3. 검증 결과 요약

| 검증 | 결과 |
| --- | --- |
| `opencode-cli.exe --version` | `opencode v2.4.1` |
| `opencode-cli.exe plugin list` | `graphify-38e661d4 (local)`, `opencode-planning-with-files 1.2.0`, `superpowers 22f0b46` — 3개 로드 |
| 전역 `opencode.json` | `provider=strata`, `model=strata/swift-iq2_xs` 변경 없음; `plugins` 2개 추가만 |
| 스킬 파일 무결성 | 4개 루트에서 수집한 **61개** SKILL.md: frontmatter 누락 0, `description` 누락 0, **ID 중복 0** (최종 검증) |
| 전역 스킬 수 | `.agents\skills` 41개 = Matt 24 + PWF 1 + Addy 11 + ECC 5 |
| Graphify | 0.9.119, 그래프 5879/18040/214, `query`/`explain` 동작, `graphify-out/` gitignore 적용 확인 |
| 실제 세션 구동 | `opencode-cli.exe run --archived --title skills-setup-verify "Reply with exactly: OK"` → 모델 응답 `OK` (플러그인 로드로 세션 시작 실패 없음). PowerShell이 진행 메시지를 stderr로 출력해 `NativeCommandError`로 표시했으나 CLI 자체 응답은 정상. |
| Git 상태 | 에이전트 변경은 `.gitignore`(graphify-out 무시), `AGENTS.md`(graphify 섹션), 신규 `docs/AI_SKILLS_SETUP.md`뿐. 사용자의 작업(`main.un` 수정, `SESSION_ARCHITECTURE.md` 삭제, 작업 중 커밋된 변경 등)은 건드리지 않음. **에이전트는 커밋하지 않음.** |

## 4. 업데이트 / 제거 방법

| 소스 | 업데이트 | 제거 |
| --- | --- | --- |
| Superpowers | `opencode-cli.exe plugin update superpowers` (또는 원하는 태그로 `plugin add ...#<tag>`) | `opencode-cli.exe plugin remove superpowers` |
| PWF 플러그인 | `opencode-cli.exe plugin update opencode-planning-with-files` | `opencode-cli.exe plugin remove opencode-planning-with-files` |
| PWF 스킬 | `npx.cmd -y skills@latest add OthmanAdi/planning-with-files -g -a opencode -y -s planning-with-files` | `npx.cmd -y skills@latest remove planning-with-files -g -a opencode` |
| Graphify | `uv tool upgrade graphifyy` + `graphify update .` | `graphify opencode uninstall --project`(스킬·플러그인·AGENTS.md 섹션 제거), `uv tool uninstall graphifyy`, `Remove-Item -Recurse graphify-out` |
| Matt / Addy / ECC 스킬 | 동일 `npx.cmd skills@latest add <repo> -g -a opencode -y -s <name>` 재실행 | `npx.cmd -y skills@latest remove <name> -g -a opencode` 또는 `Remove-Item -Recurse C:\Users\User\.agents\skills\<name>` |
| Un 전용 스킬 | `C:\dev\un\.opencode\skills\<id>\SKILL.md` 편집 | 해당 디렉터리 삭제(로컬 전용이므로 git 변경 없음) |
| 설정 롤백 | 백업 `opencode.json.bak`을 `opencode.json`으로 복사 | 동일 |

## 5. 주의사항

- `npx skills add ... -s`는 콤마/공백 분리 이름을 지원하지 않는다(`SKILLS_NO_MATCHING_SKILLS`). 한 번에 하나씩, 또는 `--skill '*'` 후 제거.
- `npm.ps1`은 실행 정책으로 차단 → 반드시 `npm.cmd` / `npx.cmd`.
- OpenCode V2의 스킬 ID는 경로에서 유도되며 대소문자를 구분한다. frontmatter의 `name`은 표시용이다.
- 스킬은 세션 시작 시 로드된다. 설치 후 새 세션(또는 reload)이 필요할 수 있다.
- 이 문서는 설치 상태 기록이며, Un 언어의 사용법은 `README.md` / `docs/README.ko.md`를 참고한다.
