# Справочник манифеста доставки

Поток доставки объявляется в собственном `Cargo.toml` приложения, в таблице
`[package.metadata.shun]` — шаблон cargo-deb / cargo-wix. Идентификация
продукта по умолчанию берётся из `[package]` (`name`, `version`); всё
остальное в таблице настраивает поток.

```toml
[package.metadata.shun]
product = "ShunDemo"                       # по умолчанию: имя пакета
publisher = "celestia-island"              # поле ARP Publisher
logo = "docs/logo.webp"                    # ресурс логотипа оболочки
payload = "examples/demo_payload"          # каталог, упаковываемый в артефакты
main-exe = "bin/shun-demo.cmd"             # точка входа внутри payload

[package.metadata.shun.install]            # цель install (по умолчанию)
local = true                               # регистрируемая установка (ARP, деинсталлятор, ярлыки)
portable = true                            # портативный режим (маркер .shun-portable, без реестра)

[package.metadata.shun.webview2]           # только Windows
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version: распакованная папка рантайма

[package.metadata.shun.flash]              # цель flash (опционально)
require-removable = true                   # отказывать несъёмным устройствам
```

## Интерфейс оболочки

`[package.metadata.shun.shell]` (ключ `shell` в отдельном документе)
настраивает оболочку времени исполнения:

```toml
[shell]
timeline = "left"          # top (горизонтальная лента) | left (вертикальная слева)
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # каналы RGB — переопределяют --color-primary
```

## Источник payload

`[package.metadata.shun.source]` выбирает, откуда payload приходит при
установке:

```toml
[source]
type = "embedded"          # архив payload встроен в бинарник установщика
```

```toml
[source]
type = "online"            # установщик сам скачивает payload
url = "https://github.com/<org>/<repo>/releases/latest/download/ShunDemo.shun"
```

Онлайн-установщик выполняет **скачивание → распаковку → проверку** одним
проходом: приходящие байты сверяются с манифестом на лету, а события
прогресса сообщают о фазах скачивания и распаковки одновременно
(многослойный прогресс). Направьте `url` на ваш канал релизов (GitHub
Releases или любой HTTP-хост) — публикация нового пакета обновляет
установщик.

## Лицензия и пользовательские шаги

```toml
license = "docs/LICENSE.md"                # markdown, показывается на шаге лицензии

[license-locales]                          # лицензии по локалям
ru = "docs/LICENSE.ru.md"
en = "docs/LICENSE.en.md"

[[custom-steps]]                           # внедрение markdown-шага
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

Интерфейс поставляется с восемью локалями (`en`, `zh-Hans`, `zh-Hant`, `ja`,
`ko`, `fr`, `ru`, `es`) и текстами по умолчанию; `shell.language = "auto"`
следует системе, фиксированная локаль закрепляет её, а по-локальные
переопределения лицензий сохраняют локализованные соглашения.
