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
desktop-shortcut = "ask"                   # always | never | ask(마법사 체크박스, 기본 선택)
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

[[package.metadata.shun.steps]]            # 순서가 있는 마법사 파이프라인(선택)
columns = 2              # 선택: 모드 그리드 열 수 (기본은 모드별 1열)
kind = "mode"                              # mode | scope | license | content | install
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

## 드라이브 루트 보호

드라이브 루트처럼 표형 파일 시스템 루트(`D:\` 외에 `D:`, UNC 공유 루트
`\\server\share`, POSIX의 `/`도 동일)가 대상이면 페이로드가 곧바로
루트에 놓이는 일은 없습니다. `InstallContext::apply_config`가 그 아래에
폴더를 한 단계 깔며, 기본값은 제품명이고 `install.root-dir-folder`로
바꿀 수 있습니다. 마법사는 루트가 선택·입력된 즉시 경로 상자를 다시
쓰므로 표시되는 대상이 항상 실제 경로이며, 헤드리스 `--dir=D:\`
실행 역시 플로우 안에서 같은 보호를 받습니다.

## 셸 UI

`[package.metadata.shun.shell]`(독립 문서에서는 `shell` 키)이 런타임 셸을
구성합니다:

```toml
[shell]
timeline = "left"          # top (상단 가로) | left (좌측 세로)
log-level = "all"          # all(기본값) | files | scripts | off
log-order = "newest"      # newest(기본값, 최신이 위) | oldest(끝에 추가)
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # RGB 채널 — --color-primary 대체
```

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

UI에는 8개 로캘(`en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`, `fr`, `ru`, `es`)의
기본 문구가 포함되어 있습니다. `shell.language = "auto"`는 시스템을 따르고,
고정 로캘을 지정하면 그 값으로 고정됩니다. 로캘별 라이선스 재정의로
현지화된 계약서도 지원합니다.
