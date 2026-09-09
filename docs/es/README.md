<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>Runtime de entrega de payloads dirigido por flujos — instaladores, flasheadores y modos portables</strong></p>

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
[Русский](../ru/README.md) ·
**Español**

</div>

---

Shun empaqueta la mitad de **entrega** de distribuir software de escritorio. Un único
documento de configuración — el propio `Cargo.toml` de la aplicación
(`[package.metadata.shun]`, el patrón cargo-deb / cargo-wix) — dirige tanto el CLI de
construcción como el shell de ejecución:

- un **payload** empaquetado una vez, incrustado en un instalador monofichero o llevado como sidecar;
- un **flujo** — elige un modo, elige un objetivo, emite progreso real;
- **targets** conectables:
  - `install` — registro estilo NSIS, por plataforma: entradas ARP de Windows, atajos (con AUMID), verbos del menú contextual del Explorador, enlaces profundos, por usuario o por máquina (autoelevación); lanzadores `.desktop` de Linux con acciones de escritorio; completado `.app` de macOS más Launch Services — y un modo portable que no escribe estado del sistema en ninguna parte;
  - `flash` — escribir una imagen en un dispositivo de bloques con verificación posterior.

El asistente en sí es un **pipeline declarativo** (`mode | scope | license | content | install`, orden libre); su panel de instalación muestra una barra de progreso real ponderada por fases y un terminal plegable que registra cada operación de fichero — la verbosidad se configura con `shell.log-level`.

En Windows el shell tiene dos caras: una interfaz WebView hikari y un **fallback egui** incrustado que no necesita WebView2 en absoluto — mismo flujo, mismo manifiesto (`--fallback` lo fuerza). Un runtime WebView2 de versión fija puede viajar dentro del payload, una copia compartida por el shell y la aplicación instalada.

## Ejemplo

Un demo cubre la entrega de punta a punta — un payload Tauri 2 real (`demo-app/`), un shell de instalación construido sobre [@celestia-island/hikari](https://github.com/celestia-island/hikari) (`shell/`), un manifiesto:

```bash
just demo                                        # # staging → build → ejecutar el shell de instalación
just demo -- --fallback                          # # forzar el shell egui sin conexión
cargo run --example demo_install                 # # generar un paquete .shun + instalación local
cargo run --example demo_install -- --portable   # # instalación portable (sin estado del sistema)
cargo run --example demo_flash                   # # enumerar dispositivos flasheables
```

Referencia completa de campos:[guía de configuración](./guides/configuration.md)
([English](../en/guides/configuration.md)).

## Estado

Versión actual: **0.2.0**. El crate se estabiliza frente a tres consumidores reales del ecosistema celestia — el shell de instalación de WoWSP, shittim-chest local y el flasheador de imágenes evernight. Las API siguen a estos tres consumidores entre versiones menores — espera cambios aditivos a partir de sus comentarios de integración.

## Estructura

| Ruta | Papel |
| --- | --- |
| `src/config.rs` | Esquema de configuración + cargador `[package.metadata.shun]` |
| `src/flow.rs` | Modelo de flujo — eventos de progreso y registro que renderiza el shell |
| `src/payload.rs` | Empaquetado del payload / manifiesto / extracción en flujo |
| `src/targets/` | Targets install (registro Windows/Linux/macOS) y flash |
| `demo-app/` | ShunDemo — la app de payload Tauri 2 (UI de ejemplo, manifiesto de entrega) |
| `shell/` | Shell de instalación: UI hikari (Tauri) + fallback egui sin conexión |
| `docs/` | Guías y notas de diseño, por idioma |

## Desarrollo

```bash
just fetch   # # preparar las recetas compartidas de celestia-devtools (una vez)
just ci      # # fmt-check + clippy + test
```

El trabajo llega a `master` mediante PRs con squash merge desde ramas `feat/*` / `fix/*`. Convenciones completas en [AGENTS.md](../../AGENTS.md).

## Licencia

SySL-1.0 — ver [LICENSE](../../LICENSE).
