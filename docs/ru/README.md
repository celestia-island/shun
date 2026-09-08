<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>Рантайм доставки payload на основе потоков — установщики, прожигатели и переносимые режимы</strong></p>

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
[한국어](../ko/README.md) ·
**Русский** ·
[Español](../es/README.md)

</div>

---

Shun упаковывает «доставляемую» половину выпуска настольного ПО. Один документ
конфигурации управляет как CLI сборки, так и оболочкой времени исполнения:

- **payload** — каталог приложения, упаковываемый один раз, встраиваемый в
  однофайловый установщик или поставляемый как sidecar;
- **flow** — выбор режима, выбор цели, трансляция реальных событий прогресса;
- подключаемые **targets**:
  - `install` — регистрация в стиле прямой регистрации Windows (запись ARP уровня пользователя,
    самокопирующий деинсталлятор, ярлык меню «Пуск», deep links) *и* портативный
    режим без единой записи в реестр;
  - `flash` — запись образа на блочное устройство с проверкой после записи.

На Windows двухвариантная стратегия WebView2 покрывает чистые машины:
стандартный артефакт требует системный рантайм, а полностью автономный артефакт
**приватно переносит рантайм WebView2 фиксированной версии** — одна копия
делится оболочкой и установленным приложением в режимах install и portable,
без прав администратора и без записей в систему.

## Пример

Одна полная демо-схема покрывает доставку от начала до конца. Полезная
нагрузка — настоящее приложение Tauri 2 (`demo-app/`, с примером
интерфейса), оболочка установителя (`shell/`, на базе
[@celestia-island/hikari](https://github.com/celestia-island/hikari))
встраивает её при сборке, и всё описывается одним манифестом доставки:

```bash
just demo                                               # подготовить демо-приложение → сборка → запуск оболочки
just demo -- --fallback                                 # принудительно офлайн-оболочка egui (без WebView2)
cargo run --example demo_flash                        # перечисление флеш-устройств
cargo run --example demo_install                      # генерирует ShunDemo.shun + локальная установка
cargo run --example demo_install -- --portable        # портативная установка (без реестра)
cargo run --example demo_install -- --uninstall       # удаление (все следы убираются)
```

`demo_install` генерирует установочный пакет `ShunDemo.shun` (zstd tar +
манифест SHA-256) в рабочем каталоге, распаковывает его с потоковым прогрессом
и — в локальном режиме — выполняет прямую регистрацию Windows. Демо-оболочка
Tauri (`shell/`, построенная на
[@celestia-island/hikari](https://github.com/celestia-island/hikari))
отображает тот же поток с полным интерфейсом, встраивая payload во время
сборки.

Манифест доставки находится в демо-crate:

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

Полный справочник полей см.
В корне полезной нагрузки остаются лишь небольшие версионируемые файлы
данных; двоичный файл приложения — продукт сборки, помещаемый в `bin/`
командой `just demo-payload` (никогда не версионируется).

[docs/en/guides/configuration.md](./docs/en/guides/configuration.md),
включая матрицу стратегий WebView2.

## Статус

Пре-релиз; crate стабилизируется против трёх реальных потребителей экосистемы
celestia — оболочки установщика WoWSP, shittim-chest local и прожигателя образов
evernight. Активная разработка идёт в ветке `dev`; `master` получит начальный
коммит релиза после завершения первого потока доставки. API нестабильны до `0.1`.

## Структура

| Путь | Роль |
| --- | --- |
| `src/config.rs` | Схема конфигурации + загрузчик `[package.metadata.shun]` |
| `src/flow.rs` | Модель потока — события прогресса, отображаемые оболочкой |
| `src/payload.rs` | Упаковка / манифест / потоковая распаковка payload |
| `src/targets/install.rs` | Цель install: бэкенды регистрации, портативный режим |
| `src/targets/flash.rs` | Цель flash: запись на блочное устройство + проверка |
| `shell/` | Оболочка установителя: UI hikari (Tauri) + офлайн-резерв egui |
| `docs/` | Руководства и заметки о дизайне, по локалям |

## Разработка

```bash
just fetch   # подключение общих рецептов celestia-devtools (один раз)
just ci      # fmt-check + clippy + test
```

Рабочий процесс: быстрая подготовка в ветке `dev`; `master` получает начальный
коммит релиза, после чего всё попадает через PR.

## Лицензия

SySL-1.0 — см. [LICENSE](./LICENSE).
