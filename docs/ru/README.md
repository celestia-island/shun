<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>Потоковый runtime доставки пейлоадов — установщики, флешеры и портативные режимы</strong></p>

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
[한국어](../ko/README.md) ·
[Français](../fr/README.md) ·
**Русский** ·
[Español](../es/README.md)

</div>

---

shun упаковывает **доставочную** половину выпуска десктоп-софта. Один
конфигурационный документ — собственный `Cargo.toml` приложения
(`[package.metadata.shun]`, паттерн cargo-deb / cargo-wix) — управляет и CLI
сборки, и runtime-оболочкой:

- **пейлоад** пакуется один раз, встраивается в однофайловый установщик или переносится как сайдкар;
- **поток** — выбрать режим, выбрать цель, стримить реальный прогресс;
- подключаемые **targets**:
  - `install` — NSIS-подобная регистрация по платформам: записи ARP в Windows, ярлыки (с AUMID), команды контекстного меню Проводника, глубокие ссылки, на пользователя или на всю машину (самоповышение); лаунчеры `.desktop` в Linux с действиями рабочего стола; достройка `.app` в macOS плюс Launch Services — и портативный режим, нигде не пишущий системного состояния;
  - `flash` — записать образ на блочное устройство с проверкой после записи.

Сам мастер — **декларативный конвейер** (`mode | scope | license | content | install`, в любом порядке); панель установки показывает настоящую взвешенную по фазам полосу прогресса и сворачиваемый терминал, протоколирующий каждую файловую операцию — многословность настраивается через `shell.log-level`.

В Windows у оболочки два лица: веб-интерфейс hikari и встроенный **egui-фолбэк**, которому WebView2 вообще не нужен — тот же поток, тот же манифест (`--fallback` принуждает). Рантайм WebView2 фиксированной версии может ехать внутри пейлоада, одна копия делится между оболочкой и установленным приложением.

## Пример

Одно демо покрывает доставку от начала до конца — настоящий пейлоад Tauri 2 (`demo-app/`), оболочка установки на [@celestia-island/hikari](https://github.com/celestia-island/hikari) (`shell/`), один манифест:

```bash
just demo                                        # # стейджинг → сборка → запуск оболочки установки
just demo -- --fallback                          # # принудительный офлайн-шелл egui
cargo run --example demo_install                 # # сгенерировать пакет .shun + локальная установка
cargo run --example demo_install -- --portable   # # портативная установка (без системного состояния)
cargo run --example demo_flash                   # # перечислить флешуемые устройства
```

Полный справочник полей:[руководство по конфигурации](./guides/configuration.md)
([English](../en/guides/configuration.md)).

## Статус

Текущий релиз: **0.2.1**. Крейт стабилизируется под трёх реальных потребителей экосистемы celestia — оболочку установки WoWSP, локальный shittim-chest и флешер образов evernight. API следуют за этими потребителями между минорными версиями — ожидайте аддитивных изменений по итогам их интеграционных отзывов.

## Структура

| Путь | Роль |
| --- | --- |
| `src/config.rs` | Схема конфигурации + загрузчик `[package.metadata.shun]` |
| `src/flow.rs` | Модель потока — события прогресса и журнала, которые рисует оболочка |
| `src/payload.rs` | Паковка пейлоада / манифест / потокное извлечение |
| `src/targets/` | Targets install (регистрация Windows/Linux/macOS) и flash |
| `demo-app/` | ShunDemo — приложение-пейлоад на Tauri 2 (пример UI, манифест доставки) |
| `shell/` | Оболочка установки: UI hikari (Tauri) + офлайн-фолбэк egui |
| `docs/` | Руководства и заметки о дизайне, по языкам |

## Разработка

```bash
just fetch   # # подготовить общие рецепты celestia-devtools (однократно)
just ci      # # fmt-check + clippy + test
```

Работа попадает в `master` через squash-merge PR из веток `feat/*` / `fix/*`. Полные конвенции — в [AGENTS.md](../../AGENTS.md).

## Лицензия

SySL-1.0 — см. [LICENSE](../../LICENSE).
