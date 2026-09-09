<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>Runtime de livraison de payloads pilotée par flux — installateurs, flasheurs et modes portables</strong></p>

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
**Français** ·
[Русский](../ru/README.md) ·
[Español](../es/README.md)

</div>

---

Shun empaquette la moitié **livraison** de la distribution de logiciels de bureau. Un seul
document de configuration — le `Cargo.toml` de l'application elle-même
(`[package.metadata.shun]`, le pattern cargo-deb / cargo-wix) — pilote à la fois le CLI de
build et le shell d'exécution :

- un **payload** empaqueté une fois, intégré dans un installateur monofichier ou porté en sidecar ;
- un **flux** — choisir un mode, choisir une cible, diffuser la progression réelle ;
- des **targets** pluggables :
  - `install` — enregistrement façon NSIS, par plateforme : entrées ARP Windows, raccourcis (avec AUMID), verbes du menu contextuel de l'Explorateur, liens profonds, par utilisateur ou machine (auto-élévation) ; lanceurs `.desktop` Linux avec actions de bureau ; complétion `.app` macOS plus Launch Services — et un mode portable qui n'écrit aucun état système nulle part ;
  - `flash` — écrire une image sur un périphérique bloc avec vérification après écriture.

L'assistant lui-même est un **pipeline déclaratif** (`mode | scope | license | content | install`, ordre libre) ; son volet d'installation affiche une vraie barre de progression pondérée par phase et un terminal repliable qui journalise chaque opération de fichier — verbosité configurable via `shell.log-level`.

Sous Windows, le shell a deux visages : une interface WebView hikari et un **fallback egui** embarqué qui n'a pas du tout besoin de WebView2 — même flux, même manifeste (`--fallback` le force). Un runtime WebView2 à version fixe peut voyager dans le payload, une copie partagée par le shell et l'application installée.

## Exemple

Une démo couvre la livraison de bout en bout — un vrai payload Tauri 2 (`demo-app/`), un shell d'installation construit sur [@celestia-island/hikari](https://github.com/celestia-island/hikari) (`shell/`), un manifeste :

```bash
just demo                                        # # staging → build → lancer le shell d'installation
just demo -- --fallback                          # # forcer le shell egui hors ligne
cargo run --example demo_install                 # # générer un paquet .shun + installation locale
cargo run --example demo_install -- --portable   # # installation portable (aucun état système)
cargo run --example demo_flash                   # # énumérer les périphériques flashables
```

Référence complète des champs :[guide de configuration](./guides/configuration.md)
([English](../en/guides/configuration.md)).

## Statut

Version actuelle : **0.2.1**. La crate se stabilise face à trois consommateurs réels de l'écosystème celestia — le shell d'installation WoWSP, shittim-chest local, et le flasheur d'images evernight. Les API suivent ces trois consommateurs entre versions mineures — attendez-vous à des changements additifs issus de leurs retours d'intégration.

## Structure

| Chemin | Rôle |
| --- | --- |
| `src/config.rs` | Schéma de configuration + chargeur `[package.metadata.shun]` |
| `src/flow.rs` | Modèle de flux — événements de progression et de journal rendus par le shell |
| `src/payload.rs` | Empaquetage du payload / manifeste / extraction en flux |
| `src/targets/` | Targets install (enregistrement Windows/Linux/macOS) et flash |
| `demo-app/` | ShunDemo — l'app payload Tauri 2 (UI d'exemple, manifeste de livraison) |
| `shell/` | Shell d'installation : UI hikari (Tauri) + fallback egui hors ligne |
| `docs/` | Guides et notes de conception, par langue |

## Développement

```bash
just fetch   # # préparer les recettes celestia-devtools partagées (une fois)
just ci      # # fmt-check + clippy + test
```

Le travail arrive sur `master` via des PR squash-mergeées depuis des branches `feat/*` / `fix/*`. Conventions complètes dans [AGENTS.md](../../AGENTS.md).

## Licence

SySL-1.0 — voir [LICENSE](../../LICENSE).
