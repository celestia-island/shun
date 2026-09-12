# Referencia del manifiesto de entrega

El flujo de entrega se declara en el `Cargo.toml` de la propia aplicación,
bajo `[package.metadata.shun]` — el patrón cargo-deb / cargo-wix. La
identidad del producto proviene por defecto de `[package]` (`name`,
`version`); el resto de la tabla personaliza el flujo.

```toml
[package.metadata.shun]
product = "ShunDemo"                       # por defecto: nombre del paquete
publisher = "celestia-island"              # campo ARP Publisher
logo = "docs/logo.webp"                    # recurso de logo del shell
payload = "examples/demo_payload"          # directorio empaquetado
main-exe = "bin/shun-demo.exe"             # punto de entrada dentro del payload

[[package.metadata.shun.attachments]]
key = "models"
title = "2D/3D model pack"
dest = "models"
[package.metadata.shun.attachments.online]
url = "https://example.test/models.shun"   # recurso adjunto opcional (las builds lite lo descargan al instalar)

[package.metadata.shun.install]            # destino install (por defecto)
local = true                               # instalación registrada (ARP, desinstalador, accesos)
portable = true                            # modo portable (marcador .shun-portable, sin registro)
portable-marker = ".shun-portable"          # nombre del archivo marcador de las copias portables (cámbialo si la app detecta el suyo)
desktop-shortcut = "ask"                   # always | never | ask (casilla del asistente, marcada por defecto)
scope = "ask"                              # user (por defecto) | machine | ask
deep-links = ["shundemo"]                  # esquemas de URL propios de la app (myapp://…)

[[package.metadata.shun.install.verbs]]    # verbos del menú contextual (verbos del Explorador / acciones de escritorio)
key = "open-data"                          # id estable del verbo
display = "Open data folder"               # texto del menú
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # solo destino app: argumentos CLI extra

[package.metadata.shun.webview2]           # solo Windows
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version: carpeta del runtime extraído

[[package.metadata.shun.steps]]            # canalización ordenada del asistente (opcional)
columns = 2              # columnas de la cuadrícula de modos (por defecto: una por modo)
kind = "mode"                              # mode | scope | license | content | install
align = "center"                           # anulación por paso: center | start (por kind por defecto)

[[package.metadata.shun.steps]]
kind = "content"
title = "Release notes"                    # los pasos content llevan un título…
markdown = "notes.md"                      # …y un documento, incrustado en tiempo de compilación

[[package.metadata.shun.steps]]
kind = "install"                           # exactamente un paso install

[package.metadata.shun.flash]              # destino flash (opcional)
require-removable = true                   # rechazar dispositivos no extraíbles
```

## Interfaz del shell

`[package.metadata.shun.shell]` (clave `shell` en un documento autónomo)
configura el shell de ejecución:

```toml
[shell]
timeline = "left"          # top (riel horizontal arriba) | left (riel vertical a la izquierda)
log-level = "all"          # all (por defecto) | files | scripts | off
log-order = "newest"      # newest (por defecto, el último arriba) | oldest (se añade al final)
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # canales RGB — sustituye --color-primary
```

## Origen del payload

`[package.metadata.shun.source]` elige de dónde viene el payload al
instalar:

```toml
[source]
type = "embedded"          # el archivo del payload va incrustado en el instalador
```

```toml
[source]
type = "online"            # el instalador descarga el payload por sí mismo
url = "https://github.com/<org>/<repo>/releases/latest/download/ShunDemo.shun"
```

Un instalador en línea ejecuta **descarga → extracción → verificación** en
una sola pasada: los bytes que llegan se verifican contra el manifiesto al
vuelo, y los eventos de progreso reportan las fases de descarga y extracción
a la vez (progreso multicapa). Apunta `url` a tu canal de publicaciones
(GitHub Releases o cualquier host HTTP): publicar un paquete nuevo actualiza
el instalador.

## Licencia y pasos personalizados

```toml
license = "docs/LICENSE.md"                # markdown, mostrado en el paso de licencia

[license-locales]                          # licencias por locale
es = "docs/LICENSE.es.md"
en = "docs/LICENSE.en.md"

[[custom-steps]]                           # inyección de un paso markdown
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

La interfaz incluye ocho locales (`en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`,
`fr`, `ru`, `es`) con textos por defecto; `shell.language = "auto"` sigue el
sistema y un locale fijo lo ancla. Las licencias por locale mantienen los
acuerdos localizados.
