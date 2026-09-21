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
start-menu-shortcut = "always"             # always | never | ask (по умолчанию always — спрашивают про ярлык на рабочем столе)
scope = "ask"                              # user (по умолчанию) | machine | ask
deep-links = ["shundemo"]                  # принадлежащие приложению схемы URL (myapp://…)
root-dir-folder = "ShunDemo"              # папка, подкладываемая под голым корнем диска (D:\ → D:\ShunDemo; по умолчанию — имя продукта)

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

## Защита от корня диска

Цель, являющаяся голым корнем файловой системы — выбранный диск вроде
`D:\` (также считаются `D:`, корень UNC-шары `\\server\share` и POSIX
`/`) — никогда не получает полезную нагрузку напрямую:
`InstallContext::apply_config` подкладывает под неё один уровень папки —
по умолчанию имя продукта, настраивается через
`install.root-dir-folder`. Мастера перезаписывают поле пути в момент
выбора или ввода корня, поэтому показываемое место назначения всегда
настоящее; headless-запуски `--dir=D:\` получают ту же защиту внутри
потока.

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

### Язык мастера

Мастер спрашивает язык на **первом шаге**: селектор со всеми восемью
поддерживаемыми локалями, каждая подписана на своём языке
(English, 简体中文, 日本語, ...). Переключение сразу перерисовывает
строки интерфейса и документы лицензии — соглашение следует выбранной
локали через `license-locales` / `locale-paths` — а выбор запоминается
для следующего запуска в файле пользовательских настроек
(`<локальные данные>/<продукт>/installer-prefs.json`, например
`%LOCALAPPDATA%\ShunDemo\installer-prefs.json` с содержимым
`{ "language": "zh-Hans" }`). Портативные запуски хранят выбор только
в памяти: переносимая копия не пишет никакого состояния системы.
`shell.language` остаётся фиксированной настройкой конфигурации —
запомненный выбор важнее неё, затем следует язык системы.

Выбранный язык попадает и в процесс установки: сценарии payload
получают его переменной окружения `SHUN_LANGUAGE`, и он записывается
в манифест установки на диске (его видят проходы ремонта/обновления).
Что именно записать в настройки самого установленного приложения,
решает сценарий payload — shun лишь экспортирует сам факт.

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

[[licenses]]                               # дополнительные документы лицензии
title = "Copyright notice"                 # необязательный заголовок над текстом
path = "NOTICE.md"                         # markdown, путь относительно манифеста
[licenses.locale-paths]                    # переопределения по локалям для документа
ru = "NOTICE.ru.md"

[[custom-steps]]                           # внедрение markdown-шага
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

`license` + `license-locales` — одно-документный сахар; массив таблиц
`licenses` объявляет дополнительные документы, каждый с необязательным
`title` и собственными `locale-paths`. Обе формы объединяются: сначала
идёт документ-сахар, затем массив в порядке объявления. Совпадающий
путь локали (`license-locales` или `locale-paths`) имеет приоритет над
базовым документом. Шаг лицензии показывает по одному документу за раз
— при нескольких документах появляются кнопки «назад/вперёд» — а один
флажок согласия покрывает их все. В разрешённом JSON каждый документ
передаётся в `licenses` (title + body); унаследованная строка `body`
объединяет все тексты через строку-разделитель, поэтому рендереры,
читающие только `body`, по-прежнему показывают соглашение целиком.

Интерфейс поставляется с восемью локалями (`en`, `zh-Hans`, `zh-Hant`, `ja`,
`ko`, `fr`, `ru`, `es`) и текстами по умолчанию; `shell.language = "auto"`
следует системе, фиксированная локаль закрепляет её, а по-локальные
переопределения лицензий сохраняют локализованные соглашения.
