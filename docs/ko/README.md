<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>플로 기반 페이로드 배포 런타임 — 설치 프로그램, 플래셔, 휴대용 모드</strong></p>

<div align="center">

[![License: SySL-1.0](https://img.shields.io/badge/License-SySL--1.0-blue.svg)](https://sysl.celestia.world)
[![Crates.io](https://img.shields.io/crates/v/shun)](https://crates.io/crates/shun)
[![docs.rs](https://docs.rs/shun/badge.svg)](https://docs.rs/shun)
[![Checks](https://img.shields.io/github/actions/workflow/status/celestia-island/shun/checks.yml)](https://github.com/celestia-island/shun/actions/workflows/checks.yml)

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

shun은 데스크톱 소프트웨어 배포의 **배포** 절반을 패키징합니다. 앱 자신의
`Cargo.toml`(`[package.metadata.shun]`, cargo-deb / cargo-wix 패턴)이라는 하나의 구성이
빌드 CLI와 런타임 셸을 모두 구동합니다:

- **페이로드**는 한 번 패키징되어 단일 파일 설치 프로그램에 내장되거나 사이드카로 휴대;
- **플로** — 모드 선택, 대상 선택, 실제 진행 상황 스트리밍;
- 플러그형 **targets**:
  - `install` — 플랫폼별 NSIS식 등록: Windows ARP 항목, 바로 가기(AUMID 포함), 탐색기 상황에 맞는 메뉴 동사, 딥 링크, 사용자별 또는 시스템 전체(자가 승격); Linux `.desktop` 런처 + 데스크톱 액션; macOS `.app` 완성 + Launch Services — 그리고 어느 플랫폼에서도 시스템 상태를 쓰지 않는 휴대용 모드;
  - `flash` — 이미지를 블록 디바이스에 쓰고 쓰기 후 검증.

마법사 자체는 **선언형 파이프라인**(`mode | scope | license | content | install`, 자유 순서)이며, 설치 화면에는 단계 가중 실제 진행률 바와 접을 수 있는 터미널이 있어 모든 파일 작업을 한 줄씩 기록합니다 — 상세도는 `shell.log-level`로 설정.

Windows에서 셸은 두 얼굴을 가집니다: hikari WebView UI와 WebView2가 전혀 필요 없는 내장 **egui 폴백** — 같은 플로, 같은 매니페스트(`--fallback`으로 강제). 고정 버전 WebView2 런타임은 페이로드에 실어갈 수 있으며 셸과 설치된 앱이 한 사본을 공유.

## 예제

하나의 demo가 배포를 종단간으로 다룹니다 — 실제 Tauri 2 페이로드(`demo-app/`), [@celestia-island/hikari](https://github.com/celestia-island/hikari) 기반 설치 셸(`shell/`), 하나의 매니페스트:

```bash
just demo                                        # # 스테이지 → 빌드 → 설치 셸 실행
just demo -- --fallback                          # # 오프라인 egui 셸 강제
cargo run --example demo_install                 # # .shun 패키지 생성 + 로컬 설치
cargo run --example demo_install -- --portable   # # 휴대용 설치(시스템 상태 없음)
cargo run --example demo_flash                   # # 플래시 후보 디바이스 나열
```

전체 필드 참조:[구성 가이드](./guides/configuration.md)
([English](../en/guides/configuration.md)).

## 상태

현재 릴리스: **0.2.1**. crate는 celestia 생태계의 세 실제 소비자 — WoWSP 설치 셸, shittim-chest 로컬, evernight 이미지 플래셔 — 에 맞춰 안정화 중입니다. API는 마이너 버전 사이에서 이 세 소비자를 따릅니다 — 통합 피드백에 따른 추가 변경을 예상.

## 구조

| 경로 | 역할 |
| --- | --- |
| `src/config.rs` | 구성 스키마 + `[package.metadata.shun]` 로더 |
| `src/flow.rs` | 플로 모델 — 셸이 그리는 진행·로그 이벤트 |
| `src/payload.rs` | 페이로드 패킹 / 매니페스트 / 스트리밍 추출 |
| `src/targets/` | 설치 target(Windows/Linux/macOS 등록)과 플래시 target |
| `demo-app/` | ShunDemo — Tauri 2 페이로드 앱(샘플 UI, 배포 매니페스트) |
| `shell/` | 설치 셸: hikari UI(Tauri) + egui 오프라인 폴백 |
| `docs/` | 언어별 가이드와 설계 노트 |

## 개발

```bash
just fetch   # # 공유 celestia-devtools 레시피 스테이징(한 번)
just ci      # # fmt-check + clippy + test
```

작업은 `feat/*` / `fix/*` 브랜치에서 squash 병합된 PR로 `master`에 반영됩니다. 전체 규약은 [AGENTS.md](../../AGENTS.md).

## 라이선스

SySL-1.0 — [LICENSE](../../LICENSE) 참조.
