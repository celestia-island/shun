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
main-exe = "bin/shun-demo.exe"             # точка входа внутри payload

[[package.metadata.shun.attachments]]
key = "models"
title = "2D/3D model pack"
dest = "models"
[package.metadata.shun.attachments.online]
url = "https://example.test/models.shun"   # необязательное вложение (в lite-сборке скачивается при установке)

[package.metadata.shun.install]            # цель install (по умолчанию)
local = true                               # регистрируемая установка (ARP, деинсталлятор, ярлыки)
portable = true                            # портативный режим (маркер .shun-portable, без реестра)
portable-marker = ".shun-portable"          # имя файла-маркера для переносимых копий (переопределите, если приложение ищет свой)
desktop-shortcut = "ask"                   # always | never | ask (флажок мастера, по умолчанию включён)
scope = "ask"                              # user (по умолчанию) | machine | ask
deep-links = ["shundemo"]                  # принадлежащие приложению схемы URL (myapp://…)

[[package.metadata.shun.install.verbs]]    # команды контекстного меню (команды Проводника / действия рабочего стола)
key = "open-data"                          # стабильный id команды
display = "Open data folder"               # текст меню
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # только для цели app: дополнительные аргументы CLI

[package.metadata.shun.webview2]           # только Windows
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version: распакованная папка рантайма

[[package.metadata.shun.steps]]            # упорядоченный конвейер мастера (опционально)
columns = 2              # число колонок сетки режимов (по умолчанию — по одной на режим)
kind = "mode"                              # mode | scope | license | content | install
align = "center"                           # переопределение шага: center | start (по умолчанию от kind)

[[package.metadata.shun.steps]]
kind = "content"
title = "Release notes"                    # шаги content имеют заголовок…
markdown = "notes.md"                      # …и документ, встраиваемый при сборке

[[package.metadata.shun.steps]]
kind = "install"                           # ровно один шаг install

[package.metadata.shun.flash]              # цель flash (опционально)
require-removable = true                   # отказывать несъёмным устройствам
```

## Интерфейс оболочки

`[package.metadata.shun.shell]` (ключ `shell` в отдельном документе)
настраивает оболочку времени исполнения:

```toml
[shell]
timeline = "left"          # top (горизонтальная лента) | left (вертикальная слева)
log-level = "all"          # all (по умолчанию) | files | scripts | off
log-order = "newest"      # newest (по умолчанию, свежие сверху) | oldest (добавление в конец)
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
