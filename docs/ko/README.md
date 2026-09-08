<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>플로우 기반 payload 전달 런타임 — 설치 관리자, 플래셔, 휴대용 모드</strong></p>

<div align="center">

[![License: SySL-1.0](https://img.shields.io/badge/License-SySL--1.0-blue.svg)](https://sysl.celestia.world)
[![GitHub](https://img.shields.io/badge/github-celestia--island%2Fshun-blue.svg)](https://github.com/celestia-island/shun)
[![Checks](https://img.shields.io/github/actions/workflow/status/celestia-island/shun/checks.yml)](https://github.com/celestia-island/shun/actions/workflows/checks.yml)
[![docs.rs](https://docs.rs/shun/badge.svg)](https://docs.rs/shun)

</div>

<div align="center">

[English](../README.md) ·
[简体中文](../zh-Hans/README.md) ·
[繁體中文](../zh-Hant/README.md) ·
[日本語](../ja/README.md) ·
**한국어** ·
[Français](../fr/README.md) ·
[Русский](../ru/README.md) ·
[Español](../es/README.md)

</div>

---

shun은 데스크톱 소프트웨어 배포의 "전달" 절반을 패키징합니다. 하나의 설정 문서가
빌드 CLI와 런타임 셸 모두를 구동합니다:

- **payload** — 앱 디렉터리를 한 번 패키징하여 단일 파일 설치 관리자에 내장하거나
  사이드카로 전달;
- **flow** — 모드 선택, 대상 선택, 실제 진행률 이벤트 스트리밍;
- 플러그인 가능한 **targets**:
  - `install` — 직접 Windows 등록(사용자 단위 ARP 항목, 자가 복사 제거 프로그램, 시작
    메뉴 바로 가기, 딥 링크) *및* 레지스트리 미사용 휴대용 모드;
  - `flash` — 블록 디바이스에 이미지 쓰기 및 사후 검증.

Windows에서 듀얼 변형 WebView2 전략이 클린 머신을 커버합니다: 표준 아티팩트는
시스템 런타임을 요구하고, 완전 자기완결형 아티팩트는 **고정 버전 WebView2 런타임을
사적으로 휴대** — 한 부의 복사본을 셸과 설치된 앱이 공유하여 설치 및 휴대 모드 전반에
걸쳐 작동하며, 관리자 권한 불필요, 시스템 쓰기 제로.

## 예제

하나의 포괄적인 데모가 배달 전 과정을 끝까지 다룹니다. payload는 실제
Tauri 2 애플리케이션(`demo-app/`, 샘플 UI 포함)이며, 설치기 셸(`shell/`,
[@celestia-island/hikari](https://github.com/celestia-island/hikari) 기반)이
빌드 시점에 이를 포함하고, 전체가 단 하나의 배달 매니페스트로 선언됩니다:

```bash
just demo                                               # 데모 앱 stage → 빌드 → 설치기 셸 실행
just demo -- --fallback                                 # 오프라인 egui 셸 강제 (WebView2 불필요)
cargo run --example demo_flash                        # 플래시 후보 장치 열거
cargo run --example demo_install                      # ShunDemo.shun 생성 + 로컬 설치
cargo run --example demo_install -- --portable        # 휴대용 설치 (레지스트리 미사용)
cargo run --example demo_install -- --uninstall       # 제거 (모든 흔적 제거)
```

`demo_install`은 설치 패키지 `ShunDemo.shun`(zstd tar + SHA-256 매니페스트)을
작업 디렉터리에 생성하고, 스트리밍 진행률로 압축을 해제하며, 로컬 모드에서는 직접 Windows
방식 등록을 수행합니다. Tauri 데모 셸(`shell/`,
[@celestia-island/hikari](https://github.com/celestia-island/hikari) 기반)이
동일한 플로우를 전체 UI로 렌더링하며 빌드 시 payload를 내장합니다.

배포 매니페스트 자체는 데모 crate에 있습니다:

```toml
[package.metadata.shun]
product = "ShunDemo"
publisher = "celestia-island"
payload = "../examples/demo_payload"
main-exe = "bin/shun-demo.exe"

[package.metadata.shun.install]
local = true
portable = true
```

전체 필드 참조는
payload 루트에는 작은 커밋된 데이터 파일만 남고, 애플리케이션 바이너리는
`just demo-payload`가 `bin/`에 stage하는 빌드 산출물입니다(커밋되지 않음).

[docs/en/guides/configuration.md](../en/guides/configuration.md)
([한국어 가이드 준비 중])을 참조하세요. WebView2 전략 매트릭스를 포함합니다.

## 상태

프리릴리스. crate는 celestia 생태계의 3개 실제 소비자 — WoWSP 설치 셸,
shittim-chest 로컬, evernight 이미지 플래셔 — 를 대상으로 안정화 중입니다. 활발한
개발은 `dev` 브랜치에서 진행되며, 첫 배포 플로우 완료 후 `master`가 초기 릴리스
커밋을 받습니다. `0.1`까지 API는 불안정합니다.

## 구조

| 경로 | 역할 |
| --- | --- |
| `src/config.rs` | 설정 스키마 + `[package.metadata.shun]` 로더 |
| `src/flow.rs` | 플로우 모델 — 셸이 렌더링하는 진행률 이벤트 |
| `src/payload.rs` | payload 패키징 / 매니페스트 / 스트리밍 추출 |
| `src/targets/install.rs` | 설치 타깃: 등록 백엔드, 휴대용 모드 |
| `src/targets/flash.rs` | 플래시 타깃: 블록 디바이스 쓰기 + 검증 |
| `shell/` | 설치기 셸: hikari UI(Tauri) + egui 오프라인 폴백 |
| `docs/` | 로캘별 가이드 및 설계 노트 |

## 개발

```bash
just fetch   # 공유 celestia-devtools 레시피 스테이징 (1회)
just ci      # fmt-check + clippy + test
```

워크플로: 빠른 준비는 `dev` 브랜치에서 진행되며, `master`가 초기 릴리스 커밋을
받은 후에는 모두 PR을 통해 병합됩니다.

## 라이선스

SySL-1.0 — [LICENSE](../LICENSE) 참조.
