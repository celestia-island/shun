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

[[package.metadata.shun.attachments]]
key = "models"
title = "2D/3D model pack"
dest = "models"
[package.metadata.shun.attachments.online]
url = "https://example.test/models.shun"   # ressource jointe optionnelle (téléchargée à l'installation par les builds lite)

[package.metadata.shun.install]            # cible install (défaut)
local = true                               # installation enregistrée (ARP, désinstalleur, raccourcis)
portable = true                            # mode portable (marqueur .shun-portable, sans registre)
portable-marker = ".shun-portable"          # nom du fichier marqueur écrit pour les copies portables (à redéfinir si l'application détecte le sien)
desktop-shortcut = "ask"                   # always | never | ask (case de l'assistant, cochée par défaut)
start-menu-shortcut = "always"             # always | never | ask (par défaut : always — la demande concerne le raccourci du bureau)
scope = "ask"                              # user (par défaut) | machine | ask
deep-links = ["shundemo"]                  # schémas d'URL détenus par l'app (myapp://…)
root-dir-folder = "ShunDemo"              # dossier inséré sous une racine de lecteur nue (D:\ → D:\ShunDemo ; défaut : nom du produit)

[[package.metadata.shun.install.verbs]]    # verbes du menu contextuel (verbes de l'Explorateur / actions de bureau)
key = "open-data"                          # id stable du verbe
display = "Open data folder"               # texte du menu
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # cible app uniquement : arguments CLI supplémentaires

[package.metadata.shun.webview2]           # Windows uniquement
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version : dossier runtime extrait

[[package.metadata.shun.steps]]            # pipeline ordonné de l'assistant (optionnel)
columns = 2              # colonnes de la grille des modes (par défaut : une par mode)
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

## Garde anti-racine

Une cible qui est une racine de système de fichiers nue — un lecteur
choisi comme `D:\` (`D:` et une racine de partage UNC
`\\server\share` comptent aussi, tout comme le `/` POSIX) — ne reçoit
jamais le payload directement : `InstallContext::apply_config` insère
un niveau de dossier dessous, le nom du produit par défaut,
personnalisable via `install.root-dir-folder`. Les assistants réécrivent
la zone de chemin dès qu'une racine est choisie ou saisie, afin que la
destination affichée soit toujours la réelle ; les exécutions headless
`--dir=D:\` bénéficient de la même garde dans le flux.

## Interface du shell

`[package.metadata.shun.shell]` (clé `shell` dans un document autonome)
configure le shell d'exécution :

```toml
[shell]
timeline = "left"          # top (rail horizontal) | left (rail vertical à gauche)
log-level = "all"          # all (par défaut) | files | scripts | off
log-order = "newest"      # newest (défaut, dernier en haut) | oldest (ajout à la fin)
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

[[licenses]]                               # documents de licence additionnels
title = "Copyright notice"                 # en-tête optionnel au-dessus du corps
path = "NOTICE.md"                         # markdown, relatif au manifeste
[licenses.locale-paths]                    # substitutions par locale de ce document
fr = "NOTICE.fr.md"

[[custom-steps]]                           # injection d'une étape markdown
key = "whats-new"
after = "license"
title = "What's New"
markdown = "docs/whats-new.md"
```

`license` + `license-locales` sont le raccourci à document unique ; le
tableau de tables `licenses` déclare des documents supplémentaires,
chacun avec un `title` optionnel et ses propres `locale-paths`. Les
deux se combinent : le document du raccourci vient en premier, puis le
tableau dans l'ordre de déclaration. Un chemin de locale correspondant
(`license-locales` ou `locale-paths`) l'emporte sur le document de
base. L'étape licence affiche un document à la fois — avec un pager
précédent/suivant quand plusieurs se résolvent — et l'unique case
d'acceptation couvre l'ensemble. Dans le JSON résolu, chaque document
est transporté sous `licenses` (title + body) ; la chaîne historique
`body` concatène tous les corps joints par une ligne de séparation,
afin que les moteurs de rendu qui ne lisent que `body` continuent
d'afficher l'accord complet.

L'interface embarque huit locales (`en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`,
`fr`, `ru`, `es`) avec leurs textes par défaut ; `shell.language = "auto"`
suit le système, une locale fixe l'épingle, et les licences par locale
restent localisées.
