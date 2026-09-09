# Référence du manifeste de livraison

Le flux de livraison se déclare dans le `Cargo.toml` de l'application, sous
`[package.metadata.shun]` — le motif cargo-deb / cargo-wix. L'identité du
produit provient par défaut de `[package]` (`name`, `version`) ; le reste de
la table personnalise le flux.

```toml
[package.metadata.shun]
product = "ShunDemo"                       # défaut : nom du paquet
publisher = "celestia-island"              # champ ARP Publisher
logo = "docs/logo.webp"                    # ressource logo du shell
payload = "examples/demo_payload"          # répertoire empaqueté
main-exe = "bin/shun-demo.exe"             # point d'entrée dans le payload

[package.metadata.shun.install]            # cible install (défaut)
local = true                               # installation enregistrée (ARP, désinstalleur, raccourcis)
portable = true                            # mode portable (marqueur .shun-portable, sans registre)
desktop-shortcut = "ask"                   # always | never | ask (case de l'assistant, cochée par défaut)
scope = "ask"                              # user (par défaut) | machine | ask
deep-links = ["shundemo"]                  # schémas d'URL détenus par l'app (myapp://…)

[[package.metadata.shun.install.verbs]]    # verbes du menu contextuel (verbes de l'Explorateur / actions de bureau)
key = "open-data"                          # id stable du verbe
display = "Open data folder"               # texte du menu
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # cible app uniquement : arguments CLI supplémentaires

[package.metadata.shun.webview2]           # Windows uniquement
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version : dossier runtime extrait

[[package.metadata.shun.steps]]            # pipeline ordonné de l'assistant (optionnel)
kind = "mode"                              # mode | scope | license | content | install
align = "center"                           # remplacement par étape : center | start (défaut du kind)

[[package.metadata.shun.steps]]
kind = "content"
title = "Release notes"                    # les étapes content portent un titre…
markdown = "notes.md"                      # …et un document, intégré à la compilation

[[package.metadata.shun.steps]]
kind = "install"                           # exactement une étape install

[package.metadata.shun.flash]              # cible flash (optionnel)
require-removable = true                   # refuser les périphériques non amovibles
```

## Interface du shell

`[package.metadata.shun.shell]` (clé `shell` dans un document autonome)
configure le shell d'exécution :

```toml
[shell]
timeline = "left"          # top (rail horizontal) | left (rail vertical à gauche)
log-level = "all"          # all (par défaut) | files | scripts | off
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # canaux RGB — remplace --color-primary
```

## Source du payload

`[package.metadata.shun.source]` choisit la provenance du payload lors de
l'installation :

```toml
[source]
type = "embedded"          # l'archive du payload est embarquée dans l'installeur
```

```toml
[source]
type = "online"            # l'installeur télécharge le payload lui-même
url = "https://github.com/<org>/<repo>/releases/latest/download/ShunDemo.shun"
```

Un installeur en ligne enchaîne **téléchargement → extraction → vérification**
en une seule passe : les octets sont vérifiés contre le manifeste à leur
arrivée, et les événements de progression rapportent simultanément les phases
de téléchargement et d'extraction (progression multi-couches). Pointez `url`
vers votre flux de publication (GitHub Releases ou tout hôte HTTP) : publier
un nouveau paquet met l'installeur à jour.

## Licence et étapes personnalisées

```toml
license = "docs/LICENSE.md"                # markdown, affiché à l'étape licence

[license-locales]                          # licences par locale
fr = "docs/LICENSE.fr.md"
en = "docs/LICENSE.en.md"

[[custom-steps]]                           # injection d'une étape markdown
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

L'interface embarque huit locales (`en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`,
`fr`, `ru`, `es`) avec leurs textes par défaut ; `shell.language = "auto"`
suit le système, une locale fixe l'épingle, et les licences par locale
restent localisées.
