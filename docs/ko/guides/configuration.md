# 배포 매니페스트 참조

배포 플로우는 애플리케이션 자신의 `Cargo.toml` 내
`[package.metadata.shun]` 테이블에서 선언합니다 — cargo-deb / cargo-wix 방식입니다.
제품 식별 정보는 `[package]`(`name`, `version`)가 기본값이며, 테이블의 나머지
항목이 배포 플로우를 사용자 지정합니다.

```toml
[package.metadata.shun]
product = "ShunDemo"                       # 기본: 패키지 이름
publisher = "celestia-island"              # ARP Publisher 필드
logo = "docs/logo.webp"                    # 셸 로고 애셋
payload = "examples/demo_payload"          # 아티팩트에 패키징할 디렉터리
main-exe = "bin/shun-demo.exe"             # payload 내 진입점

[[package.metadata.shun.attachments]]
key = "models"
title = "2D/3D model pack"
dest = "models"
[package.metadata.shun.attachments.online]
url = "https://example.test/models.shun"   # 선택 첨부 리소스(라이트 버전은 설치 시 다운로드)

[package.metadata.shun.install]            # install 타깃 (기본)
local = true                               # 등록 설치 (ARP, 제거 프로그램, 바로 가기)
portable = true                            # 휴대용 모드 (.shun-portable 마커, 레지스트리 미사용)
portable-marker = ".shun-portable"          # 휴대용 복사본에 기록되는 마커 파일 이름 (앱이 자체 마커를 감지하면 재정의)
desktop-shortcut = "ask"                   # always | never | ask(마법사 체크박스, 기본 선택; 헤드리스 실행은 ask를 바로 가기 존재 여부로 해석)
start-menu-shortcut = "always"             # always | never | ask(기본값 always, 묻는 것은 데스크톱 쪽)
scope = "ask"                              # user(기본값) | machine | ask
deep-links = ["shundemo"]                  # 앱이 소유한 URL 스킴(myapp://…)
root-dir-folder = "ShunDemo"              # 드라이브 루트 바로 아래에 자동으로 깔리는 폴더 (D:\ → D:\ShunDemo, 기본값은 제품명)

[[package.metadata.shun.install.verbs]]    # 우클릭 동사(탐색기 동사 / 데스크톱 액션)
key = "open-data"                          # 안정적인 동사 id
display = "Open data folder"               # 메뉴 텍스트
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # app 대상만: 추가 CLI 인수

[package.metadata.shun.webview2]           # Windows 전용
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version 전용: 압축 해제된 런타임 폴더
# silent-install = true                    # evergreen-installer: 런타임 없는 컴퓨터에서 동봉된
                                           #   설치 관리자를 실행(기본 켜짐 — 위자드는 먼저
                                           #   동의를 묻고, 헤드리스는 조용히 실행)
# download-url = "https://…"               # evergreen-installer: 미포함 시 설치 중 Evergreen 설치
                                           #   관리자를 내려받아 실행(동봉분과 같은 동의 절차)
# warn-missing = true                      # skip / evergreen-installer: egui 대체 화면이 시작과
                                           #   끝에 경고하고 Microsoft 공식 다운로드 페이지 링크
                                           #   제공(기본 켜짐)

[[package.metadata.shun.steps]]            # 순서가 있는 마법사 파이프라인(선택)
columns = 2              # 선택: 모드 그리드 열 수 (기본은 모드별 1열)
kind = "mode"                              # mode | scope | license | content | pairing | install
align = "center"                           # 단계별 재정의: center | start(기본은 kind에 따름)

[[package.metadata.shun.steps]]
kind = "content"
title = "Release notes"                    # content 단계는 제목을 가짐…
markdown = "notes.md"                      # …와 문서, 빌드 시 인라인

[[package.metadata.shun.steps]]
kind = "install"                           # install 단계는 정확히 하나

[package.metadata.shun.flash]              # flash 타깃 (선택)
require-removable = true                   # 이동식이 아닌 장치 거부
```

## 필드

| 필드 | 타입 | 기본값 | 의미 |
| --- | --- | --- | --- |
| `product` | string | 패키지 `name` | 제목, ARP 표시 이름, 플래시 라벨 |
| `publisher` | string | — | ARP Publisher 필드 |
| `logo` | path | — | 셸 로고 에셋(매니페스트 기준 상대 경로) |
| `payload` | path | — | 아티팩트로 패킹되는 디렉터리 |
| `main-exe` | path | — | payload 상대 진입점(바로 가기 대상) |
| `install` | table | 두 모두 켜짐 | `local` / `portable` 스위치, `portable-marker` 파일명, `desktop-shortcut` / `start-menu-shortcut` / `launch-after-install` 정책, `default-dir`(환경 변수가 전개되는 기본 위치 재정의), `verbs`, `deep-links`, `aumid`, `icon`, `root-dir-folder`(드라이브 루트 아래 깔리는 폴더, 기본은 제품명) |
| `attachments` | 테이블 배열 | 없음 | 선택적 추가 리소스(에셋 팩): `key` / `title` / `dest` / `online.url`. 라이트 빌드는 설치 시 내려받기 |
| `update` | table | 없음 | 업데이트 감시: `sources`(미러 기본 URL, 순서대로 탐색) + 첫 번째로 도달한 소스 아래에서 해석되는 `files` |
| `webview2` | table | `skip` | Windows 런타임 전략 |
| `steps` | 테이블 배열 | 기본 파이프라인 | 순서가 있는 위자드 파이프라인(`mode`/`scope`/`license`/`content`/`pairing`/`install`, 자유 순서. `install`은 정확히 하나) |
| `script` | table | 없음 | payload 스크립트 훅: `runner` + `hooks[]`(`phase`/`script`) — 스크립팅 설계 노트 참조 |
| `variant` | 테이블 배열 | 없음 | `shun build --variant`로 선택되는 이름 있는 빌드 구성 |
| `signing` | table | 미서명 | 빌드 산출물에 적용되는 Authenticode / codesign 프로필 |
| `license-sysl` | table | 없음 | SySL 라이선스와 공식 번역을 저장소에서 번들 |
| `msix.logo-background` | 색상 | `transparent` | 투명 MSIX 로고 아래 깔리는 플레이트 색 |
| `flash` | table | — | flash 타깃 선언 |

## 바로 가기 정책

`install.desktop-shortcut`와 `install.start-menu-shortcut`은 레인별로
해석됩니다. 위자드 화면은 확인란을 보여줍니다(기본 선택 — NSIS 관례).
헤드리스 레인(`--silent`)은 `ask`를 **존재**로 해석합니다: 이미 있는
바로 가기는 그 자리에서 갱신되고, 없는 바로 가기는 새로 만들지
않습니다. 조용한 업데이트가 사용자가 요청하지 않은 아이콘을 뿌려서는
안 됩니다 — AV/EDR 휴리스틱이 정확히 그것을 지목하고, 사용자가 지운
바로 가기는 지워진 채로 남아야 하기 때문입니다. `always` / `never`는
모든 레인에서 답을 고정합니다. 명시적인 `--shortcut-desktop` /
`--shortcut-menu` 플래그(업데이터 계약)는 모든 것보다 우선합니다.
제거는 바로 가기를 무조건 지웁니다.

## 설치 후 실행

`install.launch-after-install`는 완료 페이지 관례를 결정합니다: `ask`
(기본값)는 위자드의 확인란을 따릅니다 — NSIS의 "설치 후 실행"
토글이며, 헤드리스 실행에서는 선택된 것으로 간주합니다. 그런 토글이
없는 셸은 `always` / `never`로 고정할 수 있습니다.

설치 흐름 자체는 아무것도 실행하지 않습니다. 실행이 성공하면 셸이
`shun::targets::install::launch`를 호출합니다: 설치 디렉터리 안의
설정된 `main-exe`를 분리 상태로 띄웁니다(기다리지 않으므로 설치
프로세스는 바로 종료할 수 있습니다). 해석된
`InstallContext::launch_after_install`이 false이거나 진입점이
선언되지 않았으면 아무 일도 하지 않습니다. 선언된 진입점이 없으면
평소처럼 `MissingEntry` 오류가 납니다.

## 드라이브 루트 보호

드라이브 루트처럼 표형 파일 시스템 루트(`D:\` 외에 `D:`, UNC 공유 루트
`\\server\share`, POSIX의 `/`도 동일)가 대상이면 페이로드가 곧바로
루트에 놓이는 일은 없습니다. `InstallContext::apply_config`가 그 아래에
폴더를 한 단계 깔며, 기본값은 제품명이고 `install.root-dir-folder`로
바꿀 수 있습니다. 마법사는 루트가 선택·입력된 즉시 경로 상자를 다시
쓰므로 표시되는 대상이 항상 실제 경로이며, 헤드리스 `--dir=D:\`
실행 역시 플로우 안에서 같은 보호를 받습니다.

## WebView2 전략

| `type` | 동반물 | 요구 사항 | 비고 |
| --- | --- | --- | --- |
| `skip` | 없음 | 시스템 WebView2 | 표준 아티팩트 |
| `evergreen-installer` | Evergreen 오프라인 설치 관리자(약 127 MB) | 설치 시 권한 상승 | 시스템 전체 런타임 등록 |
| `fixed-version` | 압축 해제된 런타임 폴더 | 없음 | 셸과 설치된 앱이 설치/휴대용 양쪽 모드에서 공유하는 사본 |

실측(2026-09, v151 x64): fixed-version 런타임은 **압축 약 293 MB /
해제 약 661 MB** — 수용된 비용으로, 다른 패커들과 같은 수준입니다.
아티팩트가 심는 사본은 **정확히 하나**: 셸은 payload의 런타임
서브트리를 `%LOCALAPPDATA%\shun\<product>\webview2`에 스테이징하고
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`를 그곳으로 지정해 스스로
부트스트랩합니다. 해제는 해시를 인지하므로 재설치와 미리 스테이징된
파일은 다시 쓰지 않고 채택됩니다("Reusing" 진행 이벤트) — 같은
엔진의 두 번째 사본은 생기지 않습니다.

### 런타임 없는 컴퓨터의 강등

면 사다리는 런타임 부재로 죽지 않고 강등될 뿐입니다. 사다리가
해석되기 전에 셸은 로더 자신의 판정(`wry::webview_version()`,
tauri가 webview 생성 전에 거는 것과 같은 게이트)으로 런타임을
탐색하므로, 망가진 레지스트리 잔해가 webview 면을 tauri의 영어
"Could not find the WebView2 Runtime" 상자로 보내는 일은 이제
없습니다:

- `evergreen-installer` + `silent-install`(기본 `true`): 런타임
  획득은 설치 단계에 있습니다 — 초기화는 동봉된 설치 관리자(payload의
  `webview2/` 접두사에서 발견)를 사용자별 캐시로 디코드·스테이징할
  뿐입니다. 사다리 해석 전에는 아무것도 설치하지 않습니다. 위자드
  실행은 먼저 동의 확인에 멈추고("런타임 설치 / 건너뛰고 본체만"),
  승인된 경우에만 획득을 수행합니다 — 설치 관리자는 자기 창으로
  보이게 돌고, 매니페스트가 요구하면 UAC를 거칩니다. 헤드리스
  트랙은 `silent-install`을 동의로 삼아 `/silent /install`로 조용히
  실행합니다. 획득이 성공하면 머신이 그 자리에서 업그레이드되고
  로더가 사용 가능한 런타임을 보고합니다.
- 설치 관리자를 동봉하지 않지만 `download-url`을 선언한 빌드는 같은
  설치 단계 획득 안에서, 같은 동의 뒤에 Evergreen 설치 관리자를 그
  URL에서 내려받습니다(헤드리스는 `silent-install` 노브 그대로) —
  lite 맛은 자기 payload가 아니라 네트워크로 머신을 업그레이드합니다.
  느리거나 끊긴 네트워크가 첫 UI를 막는 일은 결코 없습니다: 획득은
  설치 흐름 안에서 일어나고, 강등 배너가 런타임이 오고 있다고
  말합니다.
- 결과와 무관하게 런타임이 없는 머신은 egui 면에 떨어집니다.
  `warn-missing`(기본 `true`)이고 설치 관리자를 동봉하지 않은 경우
  그 면은 위자드 시작과 끝에 한 번씩 경고하고 Microsoft 공식
  WebView2 다운로드 페이지 링크를 넣습니다. 두 노브 모두 제품별로
  끌 수 있습니다:

```toml
[package.metadata.shun.webview2]
type = "evergreen-installer"
silent-install = false                  # 동봉된 설치 관리자를 자동 실행하지 않음
warn-missing = false                    # 경고 배너 없이 조용히 강등
```

알아둘 상류 제한: tauri의 런타임 게이트는
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`를 읽지 않으므로, fixed-version
뿐(Evergreen 미설치)인 머신은 webview 생성이 가능했어도 egui 면에
떨어집니다 — 셸은 반드시 렌더링되는 면을 우선합니다.

## MSIX 로고 플레이트

Windows는 `BackgroundColor="transparent"`를 무시하는 모든 표면에서
패키지 데스크톱 앱의 로고를 기본 시스템 블루(`#0078D7`) 위에
깝니다 — App Installer 대화상자도 그중 하나입니다.
`msix.logo-background`는 `#RRGGBB` 색을 받습니다: shun CLI가 투명
로고를 그 색에 합성해 패키지 에셋으로 만들고 `BackgroundColor`로
선언합니다. 브랜드가 모든 표면의 플레이트 색을 직접 정합니다:

```toml
[package.metadata.shun.msix]
# identity-name / publisher / display-name / executable ...
logo-background = "#0F172A"
```

## 오프라인 셸

설치 셸은 hikari webview UI에 더해 egui 폴백 UI를 내장합니다. 같은
매니페스트와 payload로 구동되고(하나의 흐름, 세 개의 렌더러), 이펙트를
그리지 않으며, Windows에서 WebView2가 없을 때(배너가 없는 환경을
명시합니다) 또는 수동 강제 시 선택됩니다:

```bash
shun-installer --no-webview   # 같은 위자드, 오프라인 렌더러(--fallback은 이전 표기)
```

두 UI 모두 오프라인 스크린샷을 지원합니다 — 창 내용을 `PrintWindow`로
캡처하고(데스크톱 자동화 없음), 저장 후 프로세스가 종료됩니다:

```bash
shun-installer --screenshot=ui.png                 # hikari(webview) UI
shun-installer --no-webview --screenshot=ui.png    # egui 오프라인 UI
```

## 셸 UI

`[package.metadata.shun.shell]`(독립 문서에서는 `shell` 키)이 런타임 셸을
구성합니다:

```toml
[shell]
timeline = "left"          # top (상단 가로) | left (좌측 세로)
log-level = "all"          # all(기본값) | files | scripts | off
log-order = "newest"      # newest(기본값, 최신이 위) | oldest(끝에 추가)
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es | de | pt
faces = ["webview", "egui", "tui"]  # 이 제품 셸이 제공할 수 있는 화면(기본: 세 가지 모두)

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # RGB 채널 — --color-primary 대체
```

`shell.faces`는 이 제품의 셸이 제공할 수 있는 화면 — `webview` /
`egui` / `tui` — 을 제한합니다. 생략하면 세 가지 모두. 헤드리스
CLI 화면(`--silent`, ARP 제거 문자열)은 기반이라 비활성화할 수
없습니다. 비활성화된 화면을 요청하면 오류가 나고, 자동 선택은
비활성화된 화면을 건너뜁니다.

### 위자드 언어

위자드는 **첫 단계**에서 언어를 묻습니다. 지원하는 10개 로캘을 각각
자국어 레이블(English, 简体中文, 일본어, ...)로 나열한 선택기입니다.
언어를 바꾸면 UI 문자열과 라이선스 문서가 즉시 다시 렌더링되고
(동의 문서는 `license-locales` / `locale-paths`에 따라 선택한 로캘의
것으로 바뀜), 선택은 사용자별 기본 파일에 기억되어 다음 실행에도
이어집니다(`<로컬 앱 데이터>/<제품>/installer-prefs.json`, 예:
`%LOCALAPPDATA%\ShunDemo\installer-prefs.json`에
`{ "language": "zh-Hans" }`). 휴대용 실행에서는 선택이 메모리에만
유지됩니다. 휴대용 복사본은 시스템 상태를 전혀 쓰지 않습니다.
`shell.language`는 설정 고정값으로 남지만, 기억된 선택이 이를
덮고, 그다음은 시스템 로캘을 따릅니다.

선택한 언어는 설치 흐름으로도 전달됩니다. payload 스크립트에는
`SHUN_LANGUAGE` 환경 변수로 전달되고, 디스크의 설치 매니페스트에도
기록되어(복구/업데이트 패스에서 참조) 나중에 확인할 수 있습니다.
설치된 애플리케이션 자체 설정에 무엇을 쓸지는 payload 스크립트의
몫입니다 — shun은 사실만 내보냅니다.

## 페어링 단계

`pairing` 단계 종류는 이 머신을 위해 자격 증명을 청구합니다 — 역방향
레인: 이 머신이 코드를 표시하고 운영자가 제어 패널에 입력합니다.
하나의 창, 두 개의 레인:

```toml
[[package.metadata.shun.steps]]
kind = "pairing"

[package.metadata.shun.steps.pairing.source]
kind = "gateway"                     # gateway | scripts
official = "https://gateway.example.test"   # 미리 선택되는 게이트웨이 기본 URL
# allow-custom = true                # 사용자 지정 게이트웨이 입력도 제공(기본 켜짐)

[package.metadata.shun.steps.pairing.identity]
node-id = true                       # 게이트웨이 프로토콜의 키
name = false                         # 선택적 장치 이름(호스트명으로 미리 채움)
# tier = 1                           # 청구 시 부여되는 티어(제품이 고정할 때)

# env-file = "pairing.env"           # gateway 레인: 청구된 자격 증명이 설치
                                     #   디렉터리 안에 기록되는 위치(.env 형식)
```

- `gateway` — 내장 JSON-RPC 클라이언트(`pairing.request`가 코드를
  발급, `pairing.await`가 롱폴, `pairing.accept`가 자격 증명을
  전달). 대상 머신에 스크립트 런타임이 필요 없습니다. 이 레인은
  `node-id`와 `env-file`을 요구합니다.
- `scripts` — 제품 자체 활성화: `request` / `await` / `record`
  payload 상대 Python 스크립트(`python3`, 버전 없는 이름만 있는
  호스트에서는 `python`으로 폴백). 창의 응답은 스크립트의 **stdin**으로
  하나의 JSON 객체로 전달됩니다 — argv는 절대 쓰지 않습니다. 각
  단계의 stdout은 정확히 하나의 JSON 값이어야 합니다. `record`가
  청구 결과를 저장합니다(`env-file`은 전달되지만 소비되지 않음).
  모든 단계에는 기한이 있어 멈춘 스크립트는 기다리지 않고
  종료됩니다.

## payload 공급원

`[package.metadata.shun.source]`가 설치 시점의 payload 공급원을 선택합니다:

```toml
[source]
type = "embedded"          # payload 아카이브는 설치 본체에 내장
```

```toml
[source]
type = "online"            # 설치 프로그램이 payload를 직접 다운로드
url = "https://github.com/<org>/<repo>/releases/latest/download/ShunDemo.shun"
```

온라인 설치 관리자는 **다운로드 → 압축 해제 → 검증**을 한 번의 패스로
수행합니다: 도착하는 바이트는 매니페스트대로 검증되고, 진행 이벤트는 다운로드와
압축 해제 두 페이즈를 동시에 보고합니다(다층 진행률). `url`을 릴리스 피드
(GitHub Releases 또는 임의 HTTP 호스트)로 지정하면 새 패키지를 게시하는 것만으로
설치 관리자가 업데이트됩니다.

## 업데이트 감시

`[package.metadata.shun.update]`(독립 문서에서는 `update` 키)는 셸이
업데이트를 탐색할 미러 소스와 그 아래에서 해석되는 파일을 선언합니다:

```toml
[update]
sources = [                                # 미러 기본 URL, 순서대로 시도
  "https://mirror.example.test/shun-demo",
  "https://releases.example.test/shun-demo",
]
files = ["latest", "app-setup.exe"]
```

실행 시 셸은 `shun::update::resolve`를 호출합니다: 각 소스에 짧은
탐색(첫 파일에 대한 단순 GET, 10초 시간 제한)을 하고, 처음 `200`을
응답한 소스가 그 회 전체에서 이기며, 선언된 각 파일은 이긴 소스 아래
URL(`<source>/<file>`)로 해석됩니다. 도달 불가·비 200 소스는 경고로
건너뛰고, 아무도 응답하지 않으면 아무것도 해석되지 않습니다. 이후
셸이 해석된 URL을 스스로 가져옵니다 — `latest` 버전 표기는 텍스트로
(`fetch_text`), 산출물 다운로드는 온라인 payload 파이프라인
(`OnlinePayload`: 다운로드 → 해제 → 검증)을 거칩니다.

## 빌드 변형

`[[package.metadata.shun.variant]]`(단수 — 테이블 배열 관용구)는
`shun build --variant <name>` 플래그로 선택되는 이름 있는 산출물
구성을 선언합니다 — 설치 관리자 매트릭스의 축입니다(아키텍처와 OS는
`--target`에서, 리소스 팩은 payload 디렉터리를 탑니다):

```toml
[[package.metadata.shun.variant]]
name = "lite"                        # id + 산출물 이름 접미사
payload = "payload-lite"             # payload 디렉터리 재정의
faces = ["egui", "tui"]              # 이 변형 셸의 화면 재정의

[package.metadata.shun.variant.env]  # 셸의 cargo 빌드에 전달
SHUN_FLAVOR = "lite"                 # (build.rs 포함) — 맛 표식
```

변형 적용은 payload 디렉터리와 셸의 화면 목록을 바꿉니다. `env`는
셸 빌드에 전달되어 맛이 표식을 심을 수 있습니다. 알 수 없는 이름은
오류입니다.

## 코드 서명

`[package.metadata.shun.signing]`은 빌드 산출물의 서명을 설정합니다 —
`shun build`는 실행 끝에 설치 관리자에 서명하고 `shun sign`은 개별
파일에 서명합니다(둘 다 `--no-sign`으로 건너뜀). 미선언 = 미서명
산출물이며 로컬 테스트에 문제없습니다:

```toml
[package.metadata.shun.signing.windows]   # signtool을 통한 Authenticode
enabled = true
thumbprint = "<인증서 지문>"               # 사용자/머신 저장소에서 선택(/sha1)
# timestamp-url = "http://timestamp.digicert.com"   # RFC 3161(/tr, 기본값)

[package.metadata.shun.signing.macos]     # codesign
enabled = true
identity = "<서명 아이덴티티>"
```

## 라이선스와 사용자 지정 단계

```toml
license = "docs/LICENSE.md"                # markdown, 라이선스 단계에서 렌더링

[license-locales]                          # 로캘별 라이선스 재정의
ko = "docs/LICENSE.ko.md"
en = "docs/LICENSE.en.md"

[[licenses]]                               # 추가 라이선스 문서
title = "Copyright notice"                 # 선택: 본문 위 표제
path = "NOTICE.md"                         # markdown, 매니페스트 기준 상대 경로
[licenses.locale-paths]                    # 이 문서의 로캘별 재정의
ko = "NOTICE.ko.md"

[[custom-steps]]                           # markdown 콘텐츠 단계 주입
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

`license` + `license-locales`는 단일 문서용 축약 표기입니다. 테이블
배열 `licenses`는 문서를 추가로 선언하며, 각 문서는 선택적 `title`과
자체 `locale-paths`를 가질 수 있습니다. 둘은 결합됩니다. 축약 문서가
먼저 오고 그다음 선언 순서대로 배열이 이어집니다. 일치하는 로캘 경로
(`license-locales` 또는 `locale-paths`)는 기본 문서보다 우선합니다.
라이선스 단계는 문서를 한 번에 하나씩 표시하며, 여러 문서가 확인되면
이전/다음 페이저로 넘깁니다. 동의 체크박스 하나가 모든 문서를 대상으로
합니다. 확인된 JSON에서 각 문서는 `licenses`(title + body)로 전달되고,
기존 `body` 문자열은 모든 본문을 구분선으로 연결한 값이므로 `body`만
읽는 렌더러도 계약 전문을 그대로 표시합니다.

UI에는 10개 로캘(`en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`, `fr`, `ru`, `es`,
`de`, `pt`)의
기본 문구가 포함되어 있습니다. `shell.language = "auto"`는 시스템을 따르고,
고정 로캘을 지정하면 그 값으로 고정됩니다. 로캘별 라이선스 재정의로
현지화된 계약서도 지원합니다.

## SySL 저장소에서 라이선스 가져오기

`[package.metadata.shun.license-sysl]`은 빌드 시 라이선스 자체
저장소에서 Synthetic Source License와 공식 번역을 번들합니다
(`online` feature 필요) — 라이선스 파일을 저장소에 넣어두지 않고도
위자드가 사용자 언어로 계약을 보여줍니다:

```toml
[package.metadata.shun.license-sysl]
repo = "celestia-island/sysl"        # 기본값
branch = "main"                      # 기본값
locales = ["zh-Hans", "ja"]          # 번들할 번역(루트 영어는 항상 번들)
```

로캘 태그는 sysl 저장소의 i18n 디렉터리에 대응합니다(`zh-Hans` →
`zhs`, `zh-Hant` → `zht`. `ja`/`ko`/`fr`/`ru`/`es`/`de`/`pt`/`ar`은
직접). 번들된 문서는 다른 라이선스 문서처럼 라이선스 단계에
렌더링됩니다.

## 스크립트 훅

`[package.metadata.shun.script]`는 payload 상대 경로의 러너 스크립트를
선언하고, 배포 흐름이 단계 경계에서 실행합니다. 제품당 러너 하나 —
현재 마운트된 것은 `duckscript`(내장 인터프리터)뿐입니다:

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "post-install"               # prepare | post-install | pre-uninstall
script = "scripts/post_install.ds"   # payload 상대 경로, payload와 함께 패킹
```

실패한 훅은 흐름을 실패시킵니다. 전체 계약 — 단계, 훅이 보는 환경
(`SHUN_LANGUAGE` 포함), 예제 — 은 [스크립팅 설계 노트](../design/scripting.md)에
있습니다.
