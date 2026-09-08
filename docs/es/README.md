<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>Runtime de entrega de payload basado en flujos — instaladores, grabadores y modos portables</strong></p>

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
[Français](../fr/README.md) ·
[Русский](../ru/README.md) ·
**Español**

</div>

---

Shun empaqueta la mitad de «entrega» de la publicación de software de
escritorio. Un documento de configuración pilota tanto el CLI de construcción
como el shell de ejecución:

- un **payload** — el directorio de la aplicación empaquetado una vez,
  incrustado en un instalador mono-archivo o transportado como sidecar;
- un **flujo** — elegir un modo, elegir un destino, transmitir eventos de
  progreso reales;
- **targets** enchufables:
  - `install` — registro estilo NSIS (entrada ARP por usuario, desinstalador
    auto-copiante, acceso directo en el menú Inicio, enlaces profundos) *y*
    un modo portable sin ningún registro;
  - `flash` — escritura de una imagen en dispositivo de bloques con
    verificación posterior.

En Windows, una estrategia WebView2 de doble variante cubre máquinas limpias:
un artefacto estándar que requiere el runtime del sistema, y un artefacto
completamente autónomo que transporta **un runtime WebView2 de versión fija en
privado** — una copia compartida por el shell y la aplicación instalada a
través de los modos instalación y portable, sin admin, sin escritura en el
sistema.

## Ejemplo

Una demo integral cubre la entrega de punta a punta. La carga útil es
una aplicación Tauri 2 real (`demo-app/`, con interfaz de ejemplo), el
shell instalador (`shell/`, basado en
[@celestia-island/hikari](https://github.com/celestia-island/hikari))
la incrusta al compilar, y todo se declara en un único manifiesto:

```bash
just demo                                               # preparar la app demo → compilar → ejecutar el shell
just demo -- --fallback                                 # forzar el shell egui sin conexión (sin WebView2)
cargo run --example demo_flash                        # enumerar dispositivos grabables
cargo run --example demo_install                      # genera ShunDemo.shun + instalación local
cargo run --example demo_install -- --portable        # instalación portable (sin registro)
cargo run --example demo_install -- --uninstall       # desinstalación (borra todo rastro)
```

`demo_install` genera el paquete de instalación `ShunDemo.shun` (tar zstd +
manifiesto SHA-256) en el directorio de trabajo, lo extrae con progreso en
streaming y — en modo local — realiza el registro estilo NSIS descrito
anteriormente. El shell de demo Tauri (`shell/`, construido sobre
[@celestia-island/hikari](https://github.com/celestia-island/hikari)) muestra
el mismo flujo con una interfaz completa, incrustando el payload en tiempo de
construcción.

El manifiesto de entrega se encuentra en el crate de demo:

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

La raíz de la carga útil conserva pequeños archivos de datos versionados;
el binario de la aplicación es un producto de compilación que
`just demo-payload` coloca en `bin/` (nunca se versiona).

Vea [docs/en/guides/configuration.md](./docs/en/guides/configuration.md)
para la referencia completa, incluida la matriz de estrategias WebView2.

## Estado

Pre-lanzamiento; el crate se estabiliza frente a tres consumidores reales del
ecosistema celestia — el shell de instalación WoWSP, shittim-chest local, y el
grabador de imágenes evernight. El desarrollo activo ocurre en la rama `dev`;
`master` recibirá el commit de lanzamiento inicial una vez completado el
primer flujo de entrega. Las API son inestables hasta `0.1`.

## Estructura

| Ruta | Rol |
| --- | --- |
| `src/config.rs` | Esquema de configuración + cargador `[package.metadata.shun]` |
| `src/flow.rs` | Modelo de flujo — eventos de progreso que el shell renderiza |
| `src/payload.rs` | Empaquetado / manifiesto / extracción en streaming del payload |
| `src/targets/install.rs` | Destino de instalación: backends de registro, modo portable |
| `src/targets/flash.rs` | Destino flash: escritura en bloque + verificación |
| `shell/` | Shell instalador: UI hikari (Tauri) + reserva egui sin conexión |
| `docs/` | Guías y notas de diseño, por locale |

## Desarrollo

```bash
just fetch   # preparar recetas compartidas celestia-devtools (una vez)
just ci      # fmt-check + clippy + test
```

Flujo de trabajo: la preparación rápida ocurre en `dev`; `master` recibe el
commit de lanzamiento inicial, después de lo cual todo llega a través de PRs.

## Licencia

SySL-1.0 — ver [LICENSE](./LICENSE).
