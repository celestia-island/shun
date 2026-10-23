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
desktop-shortcut = "ask"                   # always | never | ask (casilla del asistente, marcada por defecto; headless resuelve ask según la presencia del acceso directo)
start-menu-shortcut = "always"             # always | never | ask (por defecto: always — el que se pregunta es el acceso del escritorio)
scope = "ask"                              # user (por defecto) | machine | ask
deep-links = ["shundemo"]                  # esquemas de URL propios de la app (myapp://…)
root-dir-folder = "ShunDemo"              # solo el carril de acolchado heredado (forbid-drive-root = false): carpeta acolchada bajo una raíz de unidad desnuda (D:\ → D:\ShunDemo; por defecto: nombre del producto)
# forbid-drive-root = true                 # rechazar de plano un destino en la raíz de unidad desnuda (comportamiento por defecto — ver Protección contra la raíz de unidad); false restaura el carril de acolchado anterior
# forbid-non-empty = true                  # rechazar una instalación nueva en un directorio no vacío (comportamiento por defecto — ver Reglas de ubicación de instalación); false restaura el viejo carril de «cualquier cosa vale»
# non-empty-allowlist = ["desktop.ini", "Thumbs.db", ".DS_Store"]  # nombres de basura que el veto a directorios no vacíos ignora (sin distinguir mayúsculas)
# [[package.metadata.shun.install.dir-rules]]  # reglas declarativas de ubicación de instalación (ver Reglas de ubicación de instalación)
# files = ["WorldOfWarships.exe"]              # nombres detonantes, comprobados como hijos directos del destino
# action = "block"                             # block (por defecto) | relocate
# message = "This folder holds the game client."  # motivo en inglés, mostrado tal cual
# [package.metadata.shun.install.dir-rules.message-locales]  # anulaciones por idioma del asistente (una coincidencia gana a message)
# zh-Hans = "该目录是游戏客户端。"

[[package.metadata.shun.install.verbs]]    # verbos del menú contextual (verbos del Explorador / acciones de escritorio)
key = "open-data"                          # id estable del verbo
display = "Open data folder"               # texto del menú
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # solo destino app: argumentos CLI extra

[package.metadata.shun.webview2]           # solo Windows
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version: carpeta del runtime extraído
# silent-install = true                    # evergreen-installer: ejecuta el
                                           #   instalador incluido en máquinas sin runtime
                                           #   (por defecto — el asistente pregunta primero,
                                           #   headless lo ejecuta en silencio)
# download-url = "https://…"               # evergreen-installer: sin instalador incorporado, descarga
                                           #   el instalador Evergreen durante la instalación (mismo
                                           #   consentimiento que el instalador incorporado)
# warn-missing = true                      # skip / evergreen-installer: el fallback egui avisa
                                           #   al inicio y al final con el enlace oficial de
                                           #   Microsoft (por defecto)

[[package.metadata.shun.steps]]            # canalización ordenada del asistente (opcional)
columns = 2              # columnas de la cuadrícula de modos (por defecto: una por modo)
kind = "mode"                              # mode | scope | license | content | pairing | install
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

## Campos

| Campo | Tipo | Por defecto | Significado |
| --- | --- | --- | --- |
| `product` | string | `name` del paquete | Títulos, nombre ARP, etiquetas flash |
| `publisher` | string | — | Campo Publisher de ARP |
| `logo` | path | — | Recurso de logo del shell (relativo al manifiesto) |
| `payload` | path | — | Directorio empaquetado en los artefactos |
| `main-exe` | path | — | Punto de entrada relativo al payload (destino del atajo) |
| `install` | table | ambos modos | conmutadores `local` / `portable`, nombre del marcador `portable-marker`, políticas `desktop-shortcut` / `start-menu-shortcut` / `launch-after-install`, `default-dir` (anulación de la ubicación por defecto con expansión de variables de entorno), `verbs`, `deep-links`, `aumid`, `icon`, `forbid-drive-root` (rechazar destinos en la raíz de unidad desnuda; activado por defecto — ver Protección contra la raíz de unidad), `root-dir-folder` (carpeta acolchada bajo una raíz de unidad desnuda en el carril de acolchado heredado, por defecto el nombre del producto), `forbid-non-empty` (rechazar destinos no vacíos en instalaciones nuevas; activado por defecto), `non-empty-allowlist` (nombres de basura que el veto ignora), `dir-rules` (reglas declarativas de ubicación de instalación — ver Reglas de ubicación de instalación) |
| `attachments` | array de tablas | ninguno | recursos acompañantes opcionales (packs de assets): `key` / `title` / `dest` / `online.url`; las builds lite los descargan al instalar |
| `update` | table | ninguna | vigilancia de actualizaciones: `sources` (URLs base espejo, sondeadas en orden) + `files` resueltos bajo el primer espejo alcanzable |
| `webview2` | table | `skip` | estrategia de runtime de Windows |
| `steps` | array de tablas | pipeline por defecto | pipeline ordenado del asistente (`mode`/`scope`/`license`/`content`/`pairing`/`install`, orden libre; exactamente un `install`) |
| `script` | table | ninguno | ganchos de scripts del payload: `runner` + `hooks[]` (`phase`/`script`) — ver la nota de diseño de scripting |
| `variant` | array de tablas | ninguna | configuraciones de artefacto con nombre que selecciona `shun build --variant` |
| `signing` | table | sin firma | perfil Authenticode / codesign aplicado a los artefactos |
| `license-sysl` | table | ninguna | empaqueta la licencia SySL + traducciones oficiales desde su repositorio |
| `msix.logo-background` | color | `transparent` | Color de la placa bajo un logo MSIX transparente |
| `flash` | table | — | Declara el destino flash |

## Políticas de accesos directos

`install.desktop-shortcut` e `install.start-menu-shortcut` se resuelven
según el carril. Las caras del asistente muestran casillas (marcadas por
defecto — la convención NSIS); el carril headless (`--silent`) resuelve
`ask` por **presencia**: un acceso directo que ya existe se refresca in
situ, uno que falta no se inventa. Una actualización silenciosa jamás
debe rociar iconos que el usuario no pidió — las heurísticas de AV/EDR
señalan exactamente eso, y un acceso directo que el usuario borró debe
seguir borrado. `always` / `never` fijan la respuesta en todos los
carriles; las opciones explícitas `--shortcut-desktop` /
`--shortcut-menu` (el contrato del actualizador) prevalecen sobre todo.
La desinstalación elimina los accesos directos incondicionalmente.

## Ejecutar tras la instalación

`install.launch-after-install` resuelve la convención de la página
final: `ask` (por defecto) sigue la casilla del asistente — el
conmutador NSIS de «ejecutar tras instalar», marcado en las ejecuciones
headless — mientras `always` / `never` lo fijan para shells que no
ofrecen tal conmutador.

El flujo de instalación en sí nunca lanza nada. El shell llama a
`shun::targets::install::launch` una vez completada la ejecución:
arranca el `main-exe` configurado dentro del directorio de instalación
de forma separada — sin espera, así el proceso instalador puede salir
justo después — y no hace nada cuando el
`InstallContext::launch_after_install` resuelto es falso o no hay
punto de entrada declarado. Un punto de entrada declarado pero
ausente da el error `MissingEntry` habitual.

## Protección contra la raíz de unidad

Un objetivo que sea una raíz de sistema de archivos desnuda — una
unidad elegida como `D:\` (también cuentan `D:`, una raíz de recurso
compartido UNC `\\server\share` y el `/` POSIX) — nunca recibe el
payload. Desde 0.6.30 el comportamiento por defecto es **rechazar**
la ubicación de plano: `install.forbid-drive-root` (por defecto
`true`) hace que las protecciones de directorio compartidas la
rechacen — las caras del asistente marcan la ruta en línea y bloquean
su botón Siguiente, y una ejecución headless falla de forma ruidosa —
porque un payload entregado en una raíz esparce sus archivos por ella
y una desinstalación intentaría barrer el disco entero. Poner
`forbid-drive-root = false` restaura el carril de acolchado anterior
a 0.6.30: `InstallContext::apply_config` añade un nivel de carpeta
debajo de la raíz, por defecto el nombre del producto, personalizable
con `install.root-dir-folder`, y los asistentes reescriben el cuadro
de ruta en cuanto se elige o teclea una raíz.

## Ganchos de pre-comprobación del directorio

La fase de script `check-dir` (véase la nota de diseño de scripting)
corre dentro de esas mismas protecciones, antes de escribir nada,
**solo en instalaciones nuevas**: un directorio que ya contiene una
instalación del producto (el manifiesto en disco o el ejecutable
principal están presentes) es una actualización — su decisión de
ubicación se tomó cuando aterrizó, y volver a vetarla ahora dejaría
colgadas las actualizaciones silenciosas. Un gancho veta la ubicación
asignando `SHUN_DIR_BLOCKED` a un motivo legible; el asistente
muestra el valor tal cual, así que el script lo localiza mediante
`SHUN_LANGUAGE`. Un gancho que da error tumba la comprobación con su
texto de error. La tabla declarativa `dir-rules` (ver Reglas de
ubicación de instalación) corre junto a esta fase, **antes** de los
ganchos — el veto más específico responde primero; los ganchos en sí
no cambian.

## Reglas de ubicación de instalación

Las protecciones de ubicación comparten una forma: cuanto más vacío
el destino, más segura la entrega. El conjunto por defecto es la
protección contra la raíz de unidad anterior más el **veto a
directorios no vacíos** — `install.forbid-non-empty` (por defecto
`true`) hace que una instalación nueva rechace un destino que ya
contiene archivos ajenos, porque entregar al lado de ellos enreda los
productos: el intercambio atómico de la actualización aparta el
directorio entero, y el barrido de la desinstalación lo recorre. Una
actualización (el manifiesto en disco o el ejecutable principal
presentes) siempre pasa — su decisión de ubicación se tomó cuando
aterrizó, y volver a vetarla ahora dejaría colgadas las
actualizaciones silenciosas. `install.forbid-non-empty = false`
restaura el viejo carril de «cualquier cosa vale», y
`install.non-empty-allowlist` (por defecto `["desktop.ini",
"Thumbs.db", ".DS_Store"]`, sin distinguir mayúsculas) nombra los
residuos del sistema operativo que el veto ignora: una carpeta que
solo contiene basura cuenta como vacía.

`[[package.metadata.shun.install.dir-rules]]` es el hermano basado en
datos del gancho `check-dir` — la respuesta declarativa a «esta
ubicación está mal», nacida de WoWSP: los jugadores instalaban junto
al cliente del juego, y el intercambio de la actualización habría
apartado la carpeta entera del juego:

```toml
[[package.metadata.shun.install.dir-rules]]
files = ["WorldOfWarships.exe"]     # nombres detonantes, comprobados como hijos directos del destino
action = "block"                    # block (por defecto) | relocate
message = "This folder holds the game client — pick another location."  # motivo en inglés, mostrado tal cual

[package.metadata.shun.install.dir-rules.message-locales]  # anulaciones por idioma del asistente
zh-Hans = "该目录是游戏客户端，请另选位置。"
```

- `block` (por defecto) veta **solo instalaciones nuevas** — una
  instalación que ya vive allí sigue actualizándose in situ (la regla
  de no dejar nunca colgada una instalación).
- `relocate` marca la ubicación como peligrosa para **compartir**: las
  elecciones nuevas se vetan como con `block`, y además una
  instalación existente allí queda marcada — el carril silencioso
  cede el paso al asistente interactivo en lugar de actualizar in
  situ.
- `message-locales` sustituye a la propia ramificación por
  `SHUN_LANGUAGE` del script del gancho: una entrada que coincida
  gana a `message`, que queda como respaldo en inglés.

Orden de evaluación de las protecciones para una instalación nueva —
el veto más específico responde primero: **dir-rules → ganchos de
script check-dir → veto a directorios no vacíos**; la compuerta de
ruta vacía y la protección contra la raíz de unidad responden antes
que todos, y la comprobación de escritura es incondicional. En una
actualización, solo las reglas con acción `relocate` pueden reprobar
la ubicación.

### Reubicación

El carril silencioso (`--silent --dir=<directorio antiguo>`) detecta
por la regla una instalación marcada y se relanza como asistente
interactivo con `--relocate-from <directorio antiguo>` (una opción
nueva de la CLI del shell; implica el carril interactivo y choca con
`--silent`). El asistente abre directamente en el paso de ubicación
con un aviso localizado que explica que la ubicación antigua ya no es
válida, y el directorio marcado queda excluido de las selecciones
rápidas. Tras aterrizar la instalación nueva, del directorio antiguo
solo se barren los archivos del producto listados en su manifiesto —
más el manifiesto en disco, el marcador portable y el desinstalador
copiado; los archivos ajenos (el juego anfitrión, por ejemplo) no se
tocan jamás, y el directorio antiguo en sí solo se elimina cuando
queda vacío. El registro (ARP, accesos directos) sigue a la ubicación
nueva — el barrido nunca desregistra nada. Cancelar el asistente
deja la instalación antigua intacta y actualizable — la regla de no
dejar nunca colgada una instalación se mantiene.

## Resolución de la ubicación de instalación

El destino con el que arranca una ejecución — la ruta preseleccionada
del asistente y el objetivo de la vía headless cuando la línea de
comandos no lleva `--dir` — sigue una sola prioridad: la **ubicación de
instalación registrada** del producto (la `InstallLocation` de ARP,
HKCU antes que HKLM) encabeza la cadena mientras su directorio siga
existiendo; después, la `install.default-dir` configurada y, por
último, la cadena convencional de candidatos (appdata por usuario,
Program Files, una carpeta por disco fijo). Así, una actualización
aterriza donde ya vivía la versión anterior en lugar de reubicar una
instalación con ruta personalizada en la ubicación fijada, y un
`--dir` explícito (el contrato del actualizador) sigue imponiéndose a
todo. Una ubicación registrada cuyo directorio ya no existe se ignora:
una instalación borrada no debe anclar la siguiente.

## Estrategias WebView2

| `type` | Lleva | Requiere | Notas |
| --- | --- | --- | --- |
| `skip` | nada | el WebView2 del sistema | Artefacto estándar |
| `evergreen-installer` | el instalador offline Evergreen (~127 MB) | elevación al instalar | Registra un runtime del sistema |
| `fixed-version` | la carpeta del runtime extraída | nada | Una copia privada compartida por el shell y la app instalada en los modos install y portable |

Medido (2026-09, v151 x64): el runtime fixed-version ocupa **~293 MB
comprimido / ~661 MB descomprimido** — un coste aceptado, en línea con
la salida de otros empaquetadores. El artefacto incrusta exactamente
**una** copia: el shell se autoarranca preparando el subárbol del
runtime del payload en `%LOCALAPPDATA%\shun\<product>\webview2` y
apuntando `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` allí; la extracción
conoce los hashes, así que las reinstalaciones y los archivos
preparados de antes se adoptan en vez de reescribirse (eventos de
progreso "Reusing") — no hay segunda copia del mismo motor.

### Máquinas sin runtime

La escalera de caras nunca muere por un runtime ausente — degrada.
Antes de que la escalera se resuelva, el shell sondea el runtime con el
veredicto del propio cargador (`wry::webview_version()`, la llamada
exacta sobre la que tauri condiciona la creación del webview), de modo
que un registro roto ya no puede mandar la cara webview a la caja
inglesa "Could not find the WebView2 Runtime" de tauri:

- `evergreen-installer` + `silent-install` (por defecto `true`): la
  adquisición del runtime vive en el paso de instalación — la
  inicialización solo decodifica y prepara el instalador incluido
  (descubierto bajo el prefijo `webview2/` del payload) en la caché
  por usuario, y nada se instala antes de que la escalera de caras se
  resuelva. Una ejecución del asistente se detiene primero en una
  pregunta de consentimiento («Instalar el runtime / Saltar, solo el
  programa principal») y ejecuta la adquisición solo con el permiso —
  visiblemente, con la ventana propia del instalador y elevación UAC
  cuando su manifiesto lo pide. La vía headless mantiene
  `silent-install` como consentimiento y ejecuta el instalador con
  `/silent /install`. Una adquisición con éxito actualiza la máquina
  in situ — el cargador informa de un runtime utilizable en cuanto se
  asienta.
- Una build que no lleva instalador pero declara `download-url`
  descarga el instalador Evergreen desde esa URL dentro de la misma
  adquisición del paso de instalación, tras el mismo consentimiento
  (headless mantiene el mando `silent-install`) — un sabor lite
  actualiza la máquina desde la red en vez de desde su propio payload.
  Una red lenta o ausente jamás bloquea la primera UI: la descarga
  ocurre dentro del flujo de instalación y el banner de degradación
  avisa de que el runtime viene en camino.
- Pase lo que pase, una máquina aún sin runtime toma la cara egui. Con
  `warn-missing` (por defecto `true`) y sin instalador incluido, esa
  cara avisa al principio y al final del asistente e incrusta un
  enlace a la página oficial de descarga de WebView2 de Microsoft.
  Ambos mandos se desactivan por producto:

```toml
[package.metadata.shun.webview2]
type = "evergreen-installer"
silent-install = false                  # no ejecutar nunca el instalador incluido
warn-missing = false                    # degradación silenciosa, sin banners
```

Una limitación aguas arriba que conviene conocer: la puerta de runtime
de tauri ignora `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`, así que una
máquina solo fixed-version (sin Evergreen instalado) toma la cara egui
aunque la creación del webview podría haber funcionado — el shell
prefiere la cara garantizada de renderizado.

## Placa de logo MSIX

Windows aplana el logo de una app de escritorio empaquetada sobre un
azul de sistema por defecto (`#0078D7`) en cada superficie que ignora
`BackgroundColor="transparent"` — el diálogo App Installer entre ellas.
`msix.logo-background` acepta un color `#RRGGBB`: el CLI de shun
compone el logo transparente sobre él para los assets del paquete y lo
declara como `BackgroundColor`, de modo que la marca elige el color de
la placa en todas partes:

```toml
[package.metadata.shun.msix]
# identity-name / publisher / display-name / executable ...
logo-background = "#0F172A"
```

## Shell sin conexión

El shell de instalación incrusta una UI egui de reserva además de la
UI webview hikari. La mueven el mismo manifiesto y el mismo payload
(un flujo, tres renderizadores), no dibuja efectos, y se elige cuando
falta WebView2 en Windows (el banner nombra explícitamente el entorno
ausente) o se fuerza manualmente:

```bash
shun-installer --no-webview   # mismo asistente, renderizador sin conexión (--fallback es la grafía antigua)
```

Ambas UI admiten capturas sin conexión — el contenido de la ventana se
captura con `PrintWindow`, sin automatización de escritorio, y el
proceso sale tras guardar:

```bash
shun-installer --screenshot=ui.png                 # UI hikari (webview)
shun-installer --no-webview --screenshot=ui.png    # UI egui sin conexión
```

## Interfaz del shell

`[package.metadata.shun.shell]` (clave `shell` en un documento autónomo)
configura el shell de ejecución:

```toml
[shell]
timeline = "left"          # top (riel horizontal arriba) | left (riel vertical a la izquierda)
log-level = "all"          # all (por defecto) | files | scripts | off
log-order = "newest"      # newest (por defecto, el último arriba) | oldest (se añade al final)
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es | de | pt
faces = ["webview", "egui", "tui"]  # caras que el shell de este producto puede presentar (por defecto: las tres)
silent-progress = false   # ventana de progreso pasiva para --silent (desactivada por defecto)

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # canales RGB — sustituye --color-primary
```

`shell.faces` restringe las caras que el shell de este producto puede
presentar — `webview` / `egui` / `tui`; ausente = las tres. La cara
CLI headless (`--silent`, la cadena de desinstalación ARP) es el
sustrato y no se puede desactivar. Pedir una cara desactivada es un
error; la selección automática omite las caras desactivadas.

`shell.silent-progress` dota al carril headless de una ventana de progreso pasiva: una ejecución `--silent` también muestra una ventanita siempre visible que refleja lo que dice stdout (el paso actual, el porcentaje cuando existe) — sin páginas ni clics, stdout intacto. Pensado para productos cuyo actualizador lanza el instalador desatendido: en máquinas lentas la instalación ya no se lee como «no pasó nada», y nadie lanza la versión vieja sobre el directorio en pleno intercambio. La ventana viste el lenguaje de diseño del propio shell: la vía webview la renderiza primero — una actualización desatendida ocurre, por definición, donde WebView2 ya está (la app reemplazada lo necesitaba) — con su barra de título propia, sus tokens de tema y sus textos localizados; cuando la vía webview no puede abrirse (runtime ausente, face deshabilitada, `--no-webview` forzado), la ventana egui con el mismo estilo toma el relevo. Las sesiones sin escritorio (servicios, CI) permanecen en silencio solo consola.
### Idioma del asistente

El asistente pregunta el idioma en el **primer paso**: un selector con
los diez idiomas soportados, cada uno rotulado en su propia lengua
(English, 简体中文, 日本語, ...). Al cambiarlo se vuelven a renderizar
los textos de la interfaz y los documentos de licencia al instante —
el acuerdo sigue el idioma elegido mediante `license-locales` /
`locale-paths` — y la elección se recuerda para la siguiente ejecución
en el archivo de preferencias por usuario
(`<datos locales>/<producto>/installer-prefs.json`, p. ej.
`%LOCALAPPDATA%\ShunDemo\installer-prefs.json` con
`{ "language": "zh-Hans" }`). Las ejecuciones portátiles mantienen la
elección solo en memoria: una copia portable no escribe estado del
sistema. `shell.language` sigue siendo el ajuste fijo de configuración
— una elección recordada gana sobre él, y luego el idioma del sistema.

El idioma elegido viaja con el flujo de instalación: los scripts del
payload lo reciben como variable de entorno `SHUN_LANGUAGE`, y queda
registrado en el manifiesto de instalación en disco (visible para
pasadas de reparación/actualización). Lo que se escribe en la propia
configuración de la aplicación instalada depende de los scripts del
payload — shun solo exporta el dato.

## Paso de emparejamiento

El tipo de paso `pairing` reclama una credencial para esta máquina — el
carril invertido: esta máquina muestra un código, un operador lo teclea
en un panel de control. Un panel, dos carriles:

```toml
[[package.metadata.shun.steps]]
kind = "pairing"

[package.metadata.shun.steps.pairing.source]
kind = "gateway"                     # gateway | scripts
official = "https://gateway.example.test"   # URL base de gateway preseleccionada
# allow-custom = true                # ofrecer también un campo de gateway propio (por defecto)

[package.metadata.shun.steps.pairing.identity]
node-id = true                       # el protocolo gateway se indexa por él
name = false                         # nombre de dispositivo opcional (prelleno del hostname)
# tier = 1                           # nivel provisto al reclamar, si el producto lo fija

# env-file = "pairing.env"           # carril gateway: dónde cae la credencial reclamada
                                     #   dentro del directorio de instalación (estilo .env)
```

- `gateway` — el cliente JSON-RPC integrado (`pairing.request` emite el
  código, `pairing.await` hace long-poll, `pairing.accept` entrega la
  credencial). No hace falta runtime de scripts en la máquina destino;
  el carril exige `node-id` y `env-file`.
- `scripts` — activación propia del producto: scripts Python
  `request` / `await` / `record` relativos al payload (`python3`, con
  marcha atrás a `python` si el host solo trae el nombre sin versión).
  Las respuestas del panel llegan como un único objeto JSON por el
  **stdin** del script — nunca por argv — y el stdout de cada fase debe
  ser exactamente un valor JSON; `record` persiste la reclamación (un
  `env-file` se transporta pero no se consume). Cada fase corre bajo
  un plazo: un script colgado se mata, no se espera.

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

## Vigilancia de actualizaciones

`[package.metadata.shun.update]` (clave `update` en un documento
autónomo) declara los espejos que el shell sondea en busca de
actualizaciones, y los archivos resueltos bajo ellos:

```toml
[update]
sources = [                                # URLs base espejo, probadas en orden
  "https://mirror.example.test/shun-demo",
  "https://releases.example.test/shun-demo",
]
files = ["latest", "app-setup.exe"]
```

En tiempo de ejecución el shell llama a `shun::update::resolve`: cada
espejo recibe una sonda corta (un GET simple del primer archivo,
tiempo límite 10 s), el primero que responde `200` gana toda la pasada
y cada archivo declarado se resuelve en una URL bajo la base ganadora
(`<source>/<file>`). Los espejos inalcanzables o que no responden 200
se saltan como avisos; si ninguno responde, no se resuelve nada. Luego
el shell recupera por sí mismo las URL resueltas — un marcador de
versión `latest` como texto (`fetch_text`), los artefactos por el
pipeline de payload en línea (`OnlinePayload`: descarga → extracción
→ verificación).

## Variantes de build

`[[package.metadata.shun.variant]]` (singular — la técnica del array de
tablas) declara configuraciones de artefacto con nombre que selecciona
el flag `shun build --variant <name>` — los ejes de una matriz de
instaladores (arquitectura y sistema operativo vienen de `--target`,
los packs de recursos viajan en los directorios payload):

```toml
[[package.metadata.shun.variant]]
name = "lite"                        # id + sufijo del nombre del artefacto
payload = "payload-lite"             # anulación del directorio payload
faces = ["egui", "tui"]              # caras que sustituye para el shell de la variante

[package.metadata.shun.variant.env]  # llega al cargo build del shell
SHUN_FLAVOR = "lite"                 # (y su build.rs) — marcadores de sabor
```

Aplicar una variante intercambia el directorio payload y la lista de
caras del shell; `env` viaja con la build del shell para que los
sabores puedan incrustar marcadores. Un nombre desconocido es error.

## Firma de código

`[package.metadata.shun.signing]` configura la firma de los artefactos
construidos — `shun build` firma el instalador al final de su pasada y
`shun sign` firma archivos sueltos (ambos saltan con `--no-sign`).
Ausente = artefactos sin firmar, suficiente para pruebas locales:

```toml
[package.metadata.shun.signing.windows]   # Authenticode vía signtool
enabled = true
thumbprint = "<huella del certificado>"   # elige un certificado del almacén usuario/máquina (/sha1)
# timestamp-url = "http://timestamp.digicert.com"   # RFC 3161 (/tr, el valor por defecto)

[package.metadata.shun.signing.macos]     # codesign
enabled = true
identity = "<identidad de firma>"
```

## Licencia y pasos personalizados

```toml
license = "docs/LICENSE.md"                # markdown, mostrado en el paso de licencia

[license-locales]                          # licencias por locale
es = "docs/LICENSE.es.md"
en = "docs/LICENSE.en.md"

[[licenses]]                               # documentos de licencia adicionales
title = "Copyright notice"                 # encabezado opcional sobre el cuerpo
path = "NOTICE.md"                         # markdown, relativo al manifiesto
[licenses.locale-paths]                    # sustituciones por locale de este documento
es = "NOTICE.es.md"

[[custom-steps]]                           # inyección de un paso markdown
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

`license` + `license-locales` son el atajo de documento único; el array
de tablas `licenses` declara más documentos, cada uno con un `title`
opcional y sus propios `locale-paths`. Ambos se combinan: el documento
del atajo va primero y después el array en orden de declaración. La
ruta del locale coincidente (`license-locales` o `locale-paths`) gana
sobre el documento base. El paso de licencia muestra un documento cada
vez — con un paginador anterior/siguiente cuando se resuelven varios —
y la única casilla de aceptación cubre todos. En el JSON resuelto cada
documento viaja bajo `licenses` (title + body); la cadena heredada
`body` concatena todos los cuerpos unidos por una línea divisoria, así
los renderizadores que solo leen `body` siguen mostrando el acuerdo
completo.

La interfaz incluye diez locales (`en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`,
`fr`, `ru`, `es`, `de`, `pt`) con textos por defecto; `shell.language = "auto"` sigue el
sistema y un locale fijo lo ancla. Las licencias por locale mantienen los
acuerdos localizados.

## Licencia desde el repositorio SySL

`[package.metadata.shun.license-sysl]` empaqueta en tiempo de
construcción la Synthetic Source License y sus traducciones oficiales
desde el repositorio de la propia licencia (requiere la feature
`online`) — los asistentes muestran el acuerdo en el idioma del
usuario sin vendorizar archivos de licencia:

```toml
[package.metadata.shun.license-sysl]
repo = "celestia-island/sysl"        # por defecto
branch = "main"                      # por defecto
locales = ["zh-Hans", "ja"]          # traducciones a empaquetar (el inglés raíz siempre entra)
```

Las etiquetas de locale se mapean a los directorios i18n del
repositorio sysl (`zh-Hans` → `zhs`, `zh-Hant` → `zht`;
`ja`/`ko`/`fr`/`ru`/`es`/`de`/`pt`/`ar` directos). Los documentos
empaquetados se muestran en el paso de licencia como cualquier otro
documento de licencia.

## Ganchos de scripts

`[package.metadata.shun.script]` declara scripts de runner relativos
al payload que el flujo de entrega ejecuta en las fronteras de fase.
Un runner por producto — hoy solo está montado `duckscript` (el
intérprete embebido):

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "post-install"               # prepare | post-install | pre-uninstall
script = "scripts/post_install.ds"   # relativo al payload, empaquetado con él
```

Un gancho que falla tira el flujo. El contrato completo — fases, el
entorno que ven los ganchos (`SHUN_LANGUAGE` entre otros) y ejemplos —
vive en la [nota de diseño de scripting](../design/scripting.md).
