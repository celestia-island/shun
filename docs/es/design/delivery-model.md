# Modelo de entrega

shun se encarga de la mitad de «entrega» al publicar software de escritorio.
Tres piezas ortogonales:

## Payload

Un directorio de aplicación empaquetado en un tar comprimido con zstd y un
manifiesto SHA-256 (`shun-manifest.json`). El archivo va incrustado en el
binario del instalador (`include_bytes!`, patrón de instalador mono-archivo)
o viaja como sidecar. La extracción verifica cada entrada contra el
manifiesto y emite eventos de progreso.

## Flow

Una ejecución de entrega es una secuencia de `FlowEvent` — `started`,
`progress { phase, step, percent }`, `completed`, `failed` — que la
interfaz del shell renderiza directamente. El progreso es **multicapa**: un
instalador en línea avanza a la vez las fases de descarga (download) y
extracción (extract). El flujo de instalación extrae el payload, escribe el
manifiesto en disco (lo consume la desinstalación) y luego registra (modo
local) o escribe el marcador portable (modo portable).

## Targets

- **install** — registro directo por plataforma: en Windows, una entrada ARP por usuario, un desinstalador que se auto-copia, accesos de inicio/escritorio (con AUMID) y verbos opcionales del menú contextual del Explorador; en Linux, un lanzador `.desktop` por usuario con acciones de escritorio (incluida Desinstalar); en macOS, completado del bundle `.app` más registro en Launch Services. El modo portable no toca ningún estado del sistema en ninguna plataforma. La desinstalación borra todo rastro según el manifiesto.
- **flash** — escritura en dispositivos de bloques y verificación posterior
  (grabación de imágenes). El backend llega con el flasheur evernight; la
  superficie trait y la enumeración de dispositivos están disponibles hoy.

## WebView2 (Windows)

El shell es en sí una aplicación Tauri: el runtime WebView2 es un prerrequisito
duro de su propia interfaz. El manifiesto de entrega elige la estrategia:
exigir el runtime del sistema, incrustar el instalador fuera de línea
Evergreen, o llevar en privado un runtime de versión fija — una copia
compartida por el shell y la aplicación instalada a través de los modos
install y portable.
