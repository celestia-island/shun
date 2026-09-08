<p align="center"><img src="../logo.webp" alt="Shun" width="240" /></p>

<h1 align="center">Shun</h1>

<p align="center"><strong>Runtime de livraison de payload piloté par flux — installateurs, graveurs et modes portables</strong></p>

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
**Français** ·
[Русский](../ru/README.md) ·
[Español](../es/README.md)

</div>

---

Shun prend en charge la moitié « livraison » de la publication de logiciels de
bureau. Un document de configuration pilote à la fois le CLI de construction et
le shell d'exécution :

- un **payload** — le répertoire applicatif empaqueté une fois, incrusté dans
  un installeur mono-fichier ou transporté en sidecar ;
- un **flux** — choisir un mode, choisir une cible, diffuser des événements de
  progression réels ;
- des **targets** enfichables :
  - `install` — enregistrement façon NSIS (entrée ARP par utilisateur,
    désinstalleur auto-copiant, raccourci du menu Démarrer, liens profonds)
    *et* un mode portable sans aucun registre ;
  - `flash` — écriture d'une image sur périphérique bloc avec vérification
    après écriture.

Sur Windows, une stratégie WebView2 à double variante couvre les machines
vierges : un artefact standard qui exige le runtime système, et un artefact
entièrement autonome qui transporte **un runtime WebView2 à version fixe en
privé** — une copie partagée par le shell et l'application installée, à travers
les modes installation et portable, sans admin, sans écriture système.

## Exemple

Une démo complète couvre la livraison de bout en bout. La charge utile
est une véritable application Tauri 2 (`demo-app/`, avec interface
d'exemple), le shell d'installation (`shell/`, basé sur
[@celestia-island/hikari](https://github.com/celestia-island/hikari))
l'intègre à la compilation, et l'ensemble est déclaré par un seul
manifeste de livraison :

```bash
just demo                                               # préparer l'app démo → compiler → lancer le shell
just demo -- --fallback                                 # forcer le shell egui hors ligne (sans WebView2)
cargo run --example demo_flash                        # énumérer les périphériques flashables
cargo run --example demo_install                      # génère ShunDemo.shun + installation locale
cargo run --example demo_install -- --portable        # installation portable (sans registre)
cargo run --example demo_install -- --uninstall       # désinstallation (trace effacée)
```

`demo_install` génère le paquet d'installation `ShunDemo.shun` (tar zstd +
manifeste SHA-256) dans le répertoire courant, le décompresse avec une
progression diffusée en continu et, en mode local, effectue l'enregistrement
façon NSIS décrit ci-dessus. Le shell de démo Tauri (`shell/`, construit sur
[@celestia-island/hikari](https://github.com/celestia-island/hikari)) rend le
même flux avec une interface complète, en embarquant le payload à la
construction.

Le manifeste de livraison lui-même se trouve dans le crate de démo :

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

La racine de la charge utile ne conserve que de petits fichiers de données
versionnés ; le binaire de l'application est un produit de compilation placé
dans `bin/` par `just demo-payload` (jamais versionné).

Voir [docs/en/guides/configuration.md](./docs/en/guides/configuration.md)
pour la référence complète, y compris la matrice de stratégies WebView2.

## Statut

Pré-publication ; le crate se stabilise face à trois consommateurs réels de
l'écosystème celestia — le shell d'installation WoWSP, shittim-chest local, et
le graveur d'images evernight. Le développement actif se fait sur la branche
`dev` ; `master` recevra le commit de publication initiale une fois le premier
flux de livraison terminé. Les API sont instables jusqu'à `0.1`.

## Structure

| Chemin | Rôle |
| --- | --- |
| `src/config.rs` | Schéma de configuration + chargeur `[package.metadata.shun]` |
| `src/flow.rs` | Modèle de flux — événements de progression rendus par le shell |
| `src/payload.rs` | Empaquetage / manifeste / extraction en flux du payload |
| `src/targets/install.rs` | Cible d'installation : backends d'enregistrement, mode portable |
| `src/targets/flash.rs` | Cible flash : écriture bloc + vérification |
| `shell/` | Shell d'installation : UI hikari (Tauri) + repli egui hors ligne |
| `docs/` | Guides et notes de conception, par locale |

## Développement

```bash
just fetch   # stage des recettes celestia-devtools partagées (une fois)
just ci      # fmt-check + clippy + test
```

Workflow : la préparation rapide se fait sur `dev` ; `master` reçoit le commit
de publication initiale, après quoi tout passe par des PR.

## Licence

SySL-1.0 — voir [LICENSE](./LICENSE).
