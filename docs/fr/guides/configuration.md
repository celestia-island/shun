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
desktop-shortcut = "ask"                   # always | never | ask (case de l'assistant, cochée par défaut ; headless résout ask selon la présence du raccourci)
start-menu-shortcut = "always"             # always | never | ask (par défaut : always — la demande concerne le raccourci du bureau)
scope = "ask"                              # user (par défaut) | machine | ask
deep-links = ["shundemo"]                  # schémas d'URL détenus par l'app (myapp://…)
root-dir-folder = "ShunDemo"              # seulement l'ancienne voie d'insertion (forbid-drive-root = false) : dossier intercalé sous une racine de lecteur nue (D:\ → D:\ShunDemo ; défaut : nom du produit)
# forbid-drive-root = true                 # refuser d'office une cible à la racine de lecteur nue (comportement par défaut — voir Garde anti-racine) ; false restaure la voie d'insertion ci-dessus
# forbid-non-empty = true                  # refuser une installation fraîche dans un répertoire non vide (comportement par défaut — voir Règles d'emplacement d'installation) ; false restaure l'ancienne voie « tout est permis »
# non-empty-allowlist = ["desktop.ini", "Thumbs.db", ".DS_Store"]  # noms de fichiers poubelle ignorés par le refus non-vide (sans distinction de casse)
# [[package.metadata.shun.install.dir-rules]]  # règles déclaratives d'emplacement d'installation (voir Règles d'emplacement d'installation)
# files = ["WorldOfWarships.exe"]              # noms déclencheurs, cherchés parmi les enfants directs de la cible
# action = "block"                             # block (défaut) | relocate
# message = "This folder holds the game client."  # motif en anglais, affiché tel quel
# [package.metadata.shun.install.dir-rules.message-locales]  # remplacements par langue de l'assistant (une correspondance l'emporte sur message)
# zh-Hans = "该目录是游戏客户端。"

[[package.metadata.shun.install.verbs]]    # verbes du menu contextuel (verbes de l'Explorateur / actions de bureau)
key = "open-data"                          # id stable du verbe
display = "Open data folder"               # texte du menu
target = "data-folder"                     # data-folder | uninstall | app
# arguments = "--safe"                     # cible app uniquement : arguments CLI supplémentaires

[package.metadata.shun.webview2]           # Windows uniquement
type = "skip"                              # skip | evergreen-installer | fixed-version
# path = "WebView2Runtime"                 # fixed-version : dossier runtime extrait
# silent-install = true                    # evergreen-installer : exécute
                                           #   l'installateur embarqué sans runtime (défaut —
                                           #   l'assistant demande d'abord, headless en silence)
# download-url = "https://…"               # evergreen-installer : sans installateur embarqué, récupère
                                           #   l'installateur Evergreen à l'installation (même
                                           #   consentement que l'installateur embarqué)
# warn-missing = true                      # skip / evergreen-installer : le repli egui avertit
                                           #   au début et à la fin, avec le lien officiel
                                           #   Microsoft (défaut)

[[package.metadata.shun.steps]]            # pipeline ordonné de l'assistant (optionnel)
columns = 2              # colonnes de la grille des modes (par défaut : une par mode)
kind = "mode"                              # mode | scope | license | content | pairing | install
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

## Champs

| Champ | Type | Défaut | Signification |
| --- | --- | --- | --- |
| `product` | string | `name` du paquet | Titres, nom d'affichage ARP, étiquettes flash |
| `publisher` | string | — | Champ Publisher de l'ARP |
| `logo` | path | — | Ressource logo du shell (relative au manifeste) |
| `payload` | path | — | Répertoire empaqueté dans les artefacts |
| `main-exe` | path | — | Point d'entrée relatif au payload (cible du raccourci) |
| `install` | table | les deux modes | interrupteurs `local` / `portable`, nom de fichier `portable-marker`, politiques `desktop-shortcut` / `start-menu-shortcut` / `launch-after-install`, `default-dir` (remplacement de l'emplacement par défaut, variables d'environnement expansées), `verbs`, `deep-links`, `aumid`, `icon`, `forbid-drive-root` (refuser les cibles à la racine de lecteur nue ; activé par défaut — voir Garde anti-racine), `root-dir-folder` (dossier intercalé sous une racine de lecteur nue dans l'ancienne voie d'insertion, défaut : le nom du produit), `forbid-non-empty` (refuser les cibles non vides pour les installations fraîches ; activé par défaut), `non-empty-allowlist` (noms de fichiers poubelle ignorés par le refus), `dir-rules` (règles déclaratives d'emplacement d'installation — voir Règles d'emplacement d'installation) |
| `attachments` | tableau de tables | aucun | ressources compagnes optionnelles (packs d'assets) : `key` / `title` / `dest` / `online.url` ; les builds lite les téléchargent à l'installation |
| `update` | table | aucune | veille de mise à jour : `sources` (URL de base miroirs, sondées dans l'ordre) + `files` résolus sous la première source joignable |
| `webview2` | table | `skip` | stratégie runtime Windows |
| `steps` | tableau de tables | pipeline par défaut | pipeline ordonné de l'assistant (`mode`/`scope`/`license`/`content`/`pairing`/`install`, ordre libre ; exactement un `install`) |
| `script` | table | aucun | hooks de scripts du payload : `runner` + `hooks[]` (`phase`/`script`) — voir la note de design sur le scripting |
| `variant` | tableau de tables | aucune | configurations d'artefacts nommées sélectionnées par `shun build --variant` |
| `signing` | table | non signé | profil Authenticode / codesign appliqué aux artefacts |
| `license-sysl` | table | aucune | regroupe la licence SySL + traductions officielles depuis son dépôt |
| `msix.logo-background` | couleur | `transparent` | Couleur du plateau aplati sous un logo MSIX transparent |
| `flash` | table | — | Déclare la cible flash |

## Politiques de raccourcis

`install.desktop-shortcut` et `install.start-menu-shortcut` se résolvent
selon la voie. Les faces de l'assistant montrent des cases à cocher
(cochées par défaut — la convention NSIS) ; la voie headless (`--silent`)
résout `ask` par **présence** : un raccourci déjà existant est rafraîchi
sur place, un raccourci manquant n'est pas inventé. Une mise à jour
silencieuse ne doit jamais semer des icônes que l'utilisateur n'a pas
demandées — les heuristiques AV/EDR flaggent exactement cela, et un
raccourci supprimé par l'utilisateur doit rester supprimé. `always` /
`never` épinglent la réponse dans toutes les voies ; les options
explicites `--shortcut-desktop` / `--shortcut-menu` (le contrat du
programme de mise à jour) l'emportent sur tout. La désinstallation
retire les raccourcis inconditionnellement.

## Lancement après installation

`install.launch-after-install` résout la convention de la page de fin :
`ask` (par défaut) suit la case à cocher de l'assistant — le toggle NSIS
« lancer après l'installation », coché dans les exécutions headless —
tandis que `always` / `never` l'épinglent pour les shells qui n'offrent
pas ce toggle.

Le flux d'installation lui-même ne lance jamais rien. Le shell appelle
`shun::targets::install::launch` une fois l'exécution réussie : il
démarre le `main-exe` configuré dans le répertoire d'installation, en
détaché — pas d'attente, le processus installeur peut sortir juste
après — et ne fait rien quand le `InstallContext::launch_after_install`
résolu est faux ou qu'aucun point d'entrée n'est déclaré. Un point
d'entrée déclaré mais absent remonte l'erreur `MissingEntry` habituelle.

## Garde anti-racine

Une cible qui est une racine de système de fichiers nue — un lecteur
choisi comme `D:\` (`D:` et une racine de partage UNC
`\\server\share` comptent aussi, tout comme le `/` POSIX) — ne reçoit
jamais le payload. Depuis 0.6.30, le défaut est de **refuser**
l'emplacement d'office : `install.forbid-drive-root` (défaut `true`)
fait que les gardes de répertoire partagées le rejettent — les faces
de l'assistant marquent le chemin en ligne et bloquent leur bouton
Suivant, et une exécution headless échoue bruyamment — parce qu'un
payload livré sur une racine y disperse ses fichiers et qu'une
désinstallation tenterait de balayer tout le disque. Mettre
`forbid-drive-root = false` restaure la voie d'insertion d'avant
0.6.30 : `InstallContext::apply_config` insère un niveau de dossier
sous la racine, le nom du produit par défaut, personnalisable via
`install.root-dir-folder`, et les assistants réécrivent la zone de
chemin dès qu'une racine est choisie ou saisie.

## Hooks de pré-vérification du répertoire

La phase de script `check-dir` (voir la note de design sur le
scripting) tourne dans ces mêmes gardes, avant toute écriture,
**uniquement pour les installations fraîches** : un répertoire qui
contient déjà une installation du produit (le manifeste sur disque ou
l'exécutable principal est présent) est une mise à jour — sa décision
d'emplacement a été prise à l'installation, et y opposer de nouveau
son veto maintenant laisserait les mises à jour silencieuses en plan.
Un hook met son veto à l'emplacement en affectant à `SHUN_DIR_BLOCKED`
un motif lisible ; l'assistant affiche la valeur telle quelle, le
script le localise donc via `SHUN_LANGUAGE`. Un hook qui échoue fait
échouer la vérification avec son texte d'erreur. La table déclarative
`dir-rules` (voir Règles d'emplacement d'installation) tourne aux
côtés de cette phase, **avant** les hooks — le veto le plus
spécifique répond en premier ; les hooks eux-mêmes ne changent pas.

## Règles d'emplacement d'installation

Les gardes d'emplacement partagent une forme : plus la cible est
vide, plus la livraison est sûre. L'ensemble par défaut est la garde
anti-racine ci-dessus plus le **refus non-vide** —
`install.forbid-non-empty` (défaut `true`) fait qu'une installation
fraîche refuse une cible qui contient déjà des fichiers étrangers,
car livrer à côté d'eux emmêle les produits : l'échange atomique de
la mise à jour écarte le répertoire entier, et le balayage de la
désinstallation le parcourt. Une mise à jour (le manifeste sur disque
ou l'exécutable principal présent) passe toujours — sa décision
d'emplacement a été prise à l'installation, et y opposer de nouveau
son veto laisserait les mises à jour silencieuses en plan.
`install.forbid-non-empty = false` restaure l'ancienne voie « tout
est permis », et `install.non-empty-allowlist` (défaut
`["desktop.ini", "Thumbs.db", ".DS_Store"]`, sans distinction de
casse) nomme les dépôts du système que le refus ignore : un dossier
ne contenant que des fichiers poubelle compte comme vide.

`[[package.metadata.shun.install.dir-rules]]` est le frère piloté par
données du hook `check-dir` — la réponse déclarative à « cet
emplacement ne va pas », née de WoWSP : des joueurs installaient à
côté du client du jeu, et l'échange de la mise à jour aurait écarté
le dossier entier du jeu :

```toml
[[package.metadata.shun.install.dir-rules]]
files = ["WorldOfWarships.exe"]     # noms déclencheurs, cherchés parmi les enfants directs de la cible
action = "block"                    # block (défaut) | relocate
message = "This folder holds the game client — pick another location."  # motif en anglais, affiché tel quel

[package.metadata.shun.install.dir-rules.message-locales]  # remplacements par langue de l'assistant
zh-Hans = "该目录是游戏客户端，请另选位置。"
```

- `block` (le défaut) met son veto **uniquement aux installations
  fraîches** — une installation qui vit déjà là continue de se mettre
  à jour sur place (la règle qui ne laisse jamais une installation en
  plan).
- `relocate` marque l'emplacement comme dangereux à **partager** :
  les choix neufs sont vetoisés comme avec `block`, et en plus une
  installation existante y est signalée — la voie silencieuse passe
  la main à l'assistant interactif au lieu de mettre à jour sur
  place.
- `message-locales` remplace le propre branchement `SHUN_LANGUAGE`
  du script du hook : une entrée qui correspond l'emporte sur
  `message`, qui reste le repli anglais.

Ordre d'évaluation des gardes pour une installation fraîche — le
veto le plus spécifique répond en premier : **dir-rules → hooks de
script check-dir → refus non-vide** ; le contrôle de chemin vide et
la garde anti-racine répondent avant tous, et le contrôle d'écriture
est inconditionnel. Sur une mise à jour, seules les règles à action
`relocate` peuvent rejeter l'emplacement.

### Relocalisation

La voie silencieuse (`--silent --dir=<ancien répertoire>`) détecte
via la règle une installation signalée et se relance en assistant
interactif avec `--relocate-from <ancien répertoire>` (une nouvelle
option CLI du shell ; elle implique la voie interactive et entre en
conflit avec `--silent`). La passation elle-même affiche le
motif et sort avec le code `0` sans rien installer — le contrat du
programme de mise à jour voit un succès, et le résultat appartient à
l'assistant interactif auquel l'utilisateur fait maintenant face.
Limite connue : une mise à jour silencieuse depuis une session
totalement non interactive (sans bureau utilisateur) fait naître un
assistant que personne ne voit — cette installation reste sur son
ancienne version jusqu'à ce que quelqu'un se connecte de façon
interactive. L'assistant s'ouvre directement à l'étape
d'emplacement avec un encart localisé expliquant que l'ancien
emplacement n'est plus valide, et le répertoire signalé est exclu
des sélections rapides. Une fois la nouvelle installation posée,
seuls les fichiers du produit listés dans son manifeste sont balayés
hors de l'ancien répertoire — plus le manifeste sur disque, le
marqueur portable et le désinstalleur copié ; les fichiers étrangers
(le jeu hôte, par exemple) ne sont jamais touchés, et l'ancien
répertoire lui-même n'est supprimé qu'une fois vide. L'enregistrement
(ARP, raccourcis) suit le nouvel emplacement — le balayage ne
désenregistre jamais rien. Annuler l'assistant laisse l'ancienne
installation installée et tournant sur son ancienne version — mais
elle ne peut plus se mettre à jour silencieusement sur place :
chaque tentative silencieuse future redéclenche la passation de la
relocalisation et l'assistant s'ouvre à nouveau, jusqu'à ce que
l'utilisateur complète la relocalisation. La règle qui ne laisse
jamais une installation en plan tient — l'installation n'est pas
cassée ; ses mises à jour silencieuses attendent simplement le
déménagement.

## Résolution de l'emplacement d'installation

La destination de départ d'une exécution — le chemin présélectionné
de l'assistant et la cible de la voie headless quand la ligne de
commande ne porte pas de `--dir` — suit une seule priorité : la
**localisation d'installation enregistrée** du produit (l'
`InstallLocation` ARP, HKCU avant HKLM) mène la chaîne tant que son
répertoire existe encore, puis la `install.default-dir` configurée,
puis la chaîne conventionnelle de candidats (appdata par utilisateur,
Program Files, un dossier par disque fixe). Une mise à jour atterrit
donc là où vivait déjà la version précédente au lieu de déplacer une
installation au chemin personnalisé vers l'emplacement épinglé, et un
`--dir` explicite (le contrat du moteur de mise à jour) l'emporte
toujours sur tout. Une localisation enregistrée dont le répertoire
n'existe plus est ignorée — une installation supprimée ne doit pas
épingler la suivante.

## Stratégies WebView2

| `type` | Embarque | Exige | Notes |
| --- | --- | --- | --- |
| `skip` | rien | le WebView2 système | Artefact standard |
| `evergreen-installer` | l'installateur hors-ligne Evergreen (~127 Mo) | élévation à l'installation | Enregistre un runtime système |
| `fixed-version` | le dossier runtime extrait | rien | Une copie privée partagée par le shell et l'app installée, en modes install et portable |

Mesuré (2026-09, v151 x64) : le runtime fixed-version fait **~293 Mo
comprimé / ~661 Mo décompressé** — un coût accepté, dans la norme des
autres packagers. L'artefact n'embarque qu'**une seule** copie : le shell
s'auto-amorce en stagant le sous-arbre runtime du payload dans
`%LOCALAPPDATA%\shun\<product>\webview2` et en pointant
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` dessus ; l'extraction connaît les
hashs, donc les réinstallations et fichiers pré-stagés sont adoptés au
lieu d'être réécrits (événements de progression « Reusing ») — pas de
deuxième copie du même moteur.

### Machines sans runtime

L'échelle des faces ne meurt jamais sur un runtime manquant — elle
dégrade. Avant que l'échelle se résolve, le shell sonde le runtime avec
le verdict du loader (`wry::webview_version()`, l'appel exact sur lequel
tauri conditionne la création du webview), si bien qu'un état de
registre cassé ne peut plus envoyer la face webview dans la boîte
anglaise « Could not find the WebView2 Runtime » de tauri :

- `evergreen-installer` + `silent-install` (défaut `true`) :
  l'acquisition du runtime vit dans l'étape d'installation —
  l'initialisation ne fait que décoder et stager l'installateur embarqué
  (découvert sous le préfixe `webview2/` du payload) dans le cache par
  utilisateur, et rien ne s'installe avant que l'échelle des faces se
  résolve. Une exécution assistant marque d'abord une pause sur une
  demande de consentement (« Installer le runtime / Passer, programme
  principal seulement ») et ne lance l'acquisition que sur accord —
  visiblement, avec la fenêtre propre de l'installateur et l'élévation
  UAC quand son manifeste l'exige. La voie headless garde
  `silent-install` comme consentement et exécute l'installateur avec
  `/silent /install`. Une acquisition réussie met la machine à niveau
  en place — le loader rapporte un runtime utilisable une fois
  stabilisé.
- Un build qui n'embarque aucun installateur mais déclare `download-url`
  récupère l'installateur Evergreen depuis cette URL dans la même
  acquisition de l'étape d'installation, derrière le même consentement
  (headless garde le réglage `silent-install`) — une saveur lite met la
  machine à niveau depuis le réseau plutôt que depuis son propre
  payload. Un réseau lent ou absent ne bloque jamais la première UI :
  la récupération se passe dans le flux d'installation, et la bannière
  de dégradation annonce que le runtime arrive.
- Quoi qu'il arrive, une machine encore sans runtime prend la face egui.
  Avec `warn-missing` (défaut `true`) et aucun installateur embarqué,
  cette face avertit au début et à la fin de l'assistant et embarque un
  lien vers la page officielle de téléchargement WebView2 de Microsoft.
  Les deux réglages se désactivent par produit :

```toml
[package.metadata.shun.webview2]
type = "evergreen-installer"
silent-install = false                  # ne jamais exécuter l'installateur embarqué
warn-missing = false                    # dégradation silencieuse, sans bannières
```

Une limite amont à connaître : la porte runtime de tauri ignore
`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`, donc une machine fixed-version
seule (sans Evergreen installé) prend la face egui même si la création
du webview aurait pu réussir — le shell préfère la face garantie
affichable.

## Plateau de logo MSIX

Windows plaque le logo d'une app desktop empaquetée sur un bleu système
par défaut (`#0078D7`) sur chaque surface qui ignore
`BackgroundColor="transparent"` — la boîte de dialogue App Installer
entre autres. `msix.logo-background` prend une couleur `#RRGGBB` : le
CLI shun composite un logo transparent dessus pour les assets du
paquet et la déclare comme `BackgroundColor`, si bien que la marque
choisit la couleur du plateau partout :

```toml
[package.metadata.shun.msix]
# identity-name / publisher / display-name / executable ...
logo-background = "#0F172A"
```

## Shell hors ligne

Le shell d'installation embarque une UI egui de repli en plus de l'UI
webview hikari. Pilotée par le même manifeste et le même payload (un
flux, trois moteurs de rendu), sans effets, elle est choisie quand
WebView2 manque sur Windows (la bannière nomme explicitement
l'environnement manquant) ou forcée manuellement :

```bash
shun-installer --no-webview   # même assistant, moteur hors ligne (--fallback est l'orthographe historique)
```

Les deux UI supportent la capture hors ligne — le contenu de la fenêtre
est saisi avec `PrintWindow`, sans automatisation de bureau, et le
processus sort après l'enregistrement :

```bash
shun-installer --screenshot=ui.png                 # UI hikari (webview)
shun-installer --no-webview --screenshot=ui.png    # UI egui hors ligne
```

## Interface du shell

`[package.metadata.shun.shell]` (clé `shell` dans un document autonome)
configure le shell d'exécution :

```toml
[shell]
timeline = "left"          # top (rail horizontal) | left (rail vertical à gauche)
log-level = "all"          # all (par défaut) | files | scripts | off
log-order = "newest"      # newest (défaut, dernier en haut) | oldest (ajout à la fin)
language = "auto"          # auto | en | zh-Hans | zh-Hant | ja | ko | fr | ru | es | de | pt
faces = ["webview", "egui", "tui"]  # faces que le shell de ce produit peut présenter (défaut : les trois)
silent-progress = false   # fenêtre de progression passive pour --silent (désactivée par défaut)

[shell.theme]
mode = "system"            # system | light | dark
accent = [34, 211, 238]    # canaux RGB — remplace --color-primary
```

`shell.faces` restreint les faces que le shell de ce produit peut
présenter — `webview` / `egui` / `tui` ; absent = les trois. La face
CLI headless (`--silent`, la chaîne de désinstallation ARP) est le
substrat et ne peut pas être désactivée. Demander une face désactivée
est une erreur ; la sélection automatique saute les faces désactivées.

`shell.silent-progress` dote la voie headless d'une fenêtre de progression passive : une exécution `--silent` affiche aussi une petite fenêtre toujours au premier plan qui reflète ce que dit stdout (l'étape en cours, le pourcentage quand il existe) — pas de pages, pas de clics, stdout inchangé. Pensé pour les produits dont l'updater lance l'installateur sans surveillance : sur une machine lente l'installation ne se lit plus comme « rien ne s'est passé », et personne ne lance l'ancienne version dans le répertoire en plein échange. La fenêtre habille le langage graphique du shell : la voie webview la rend d'abord — une mise à jour sans surveillance se produit par définition là où WebView2 est déjà présent (l'appli remplacée en avait besoin) — avec sa barre de titre dédiée, ses jetons de thème et ses textes localisés ; quand la voie webview ne peut pas s'ouvrir (runtime absent, face désactivée, `--no-webview` forcé), la fenêtre egui stylée prend le relais. Les sessions sans bureau (services, CI) restent silencieusement en console seule.
### Langue de l'assistant

L'assistant demande sa langue à la **première étape** : un sélecteur
listant les dix langues prises en charge, chacune libellée dans sa
propre langue (English, 简体中文, 日本語, ...). Le changement re-rend
immédiatement les textes de l'interface et les documents de licence —
l'accord suit la langue choisie via `license-locales` / `locale-paths`
— et le choix est mémorisé pour l'exécution suivante dans le fichier de
préférences par utilisateur
(`<données locales>/<produit>/installer-prefs.json`, p. ex.
`%LOCALAPPDATA%\ShunDemo\installer-prefs.json` contenant
`{ "language": "zh-Hans" }`). Les exécutions portables gardent le choix
en mémoire seule : une copie portable n'écrit aucun état système.
`shell.language` reste la valeur fixée par la configuration — un choix
mémorisé l'emporte dessus, puis la langue du système.

La langue choisie accompagne le flux d'installation : les scripts du
payload la reçoivent comme variable d'environnement `SHUN_LANGUAGE`,
et elle est consignée dans le manifeste d'installation sur disque
(visible pour les passes de réparation/mise à jour). Ce qui est écrit
dans la configuration propre à l'application installée relève des
scripts du payload — shun n'exporte que l'information.

## Étape d'appairage

Le type d'étape `pairing` réclame un identifiant pour cette machine —
la voie inversée : cette machine affiche un code, un opérateur le tape
dans un panneau de contrôle. Un volet, deux voies :

```toml
[[package.metadata.shun.steps]]
kind = "pairing"

[package.metadata.shun.steps.pairing.source]
kind = "gateway"                     # gateway | scripts
official = "https://gateway.example.test"   # URL de base de passerelle présélectionnée
# allow-custom = true                # offrir aussi une saisie de passerelle (défaut)

[package.metadata.shun.steps.pairing.identity]
node-id = true                       # le protocole passerelle s'y indexe
name = false                         # nom de périphérique optionnel (prérempli du hostname)
# tier = 1                           # palier provisionné à la réclamation, si le produit l'épingle

# env-file = "pairing.env"           # voie gateway : où l'identifiant réclamé atterrit
                                     #   dans le répertoire d'installation (style .env)
```

- `gateway` — le client JSON-RPC intégré (`pairing.request` émet le
  code, `pairing.await` scrute en long-poll, `pairing.accept` livre
  l'identifiant). Aucun runtime de script requis sur la machine cible ;
  la voie exige `node-id` et `env-file`.
- `scripts` — activation propriétaire : scripts Python `request` /
  `await` / `record` relatifs au payload (`python3`, repli sur `python`
  si l'hôte n'a que le nom sans version). Les réponses du volet
  arrivent en un seul objet JSON sur l'**stdin** du script — jamais en
  argv — et le stdout de chaque phase doit être exactement une valeur
  JSON ; `record` persiste la réclamation (un `env-file` est transporté
  mais non consommé). Chaque phase court sous une échéance : un script
  bloqué est tué, pas attendu.

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

## Veille de mise à jour

`[package.metadata.shun.update]` (clé `update` dans un document
autonome) déclare les miroirs que le shell sonde pour les mises à jour,
et les fichiers résolus dessous :

```toml
[update]
sources = [                                # URL de base miroirs, essayées dans l'ordre
  "https://mirror.example.test/shun-demo",
  "https://releases.example.test/shun-demo",
]
files = ["latest", "app-setup.exe"]
```

Au runtime, le shell appelle `shun::update::resolve` : chaque source
reçoit une courte sonde (un simple GET du premier fichier, délai 10 s),
la première qui répond `200` gagne pour toute la passe, et chaque
fichier déclaré se résout en une URL sous la base gagnante
(`<source>/<file>`). Les sources injoignables ou non-200 sont sautées
en avertissements ; si aucune ne répond, rien ne se résout. Le shell
récupère ensuite lui-même les URL résolues — un marqueur `latest` en
texte (`fetch_text`), les artefacts via le pipeline de payload en ligne
(`OnlinePayload` : téléchargement → extraction → vérification).

## Variantes de build

`[[package.metadata.shun.variant]]` (singulier — l'idiome du tableau de
tables) déclare des configurations d'artefacts nommées que sélectionne
le drapeau `shun build --variant <name>` — les axes d'une matrice
d'installateurs (l'architecture et l'OS viennent de `--target`, les
packs de ressources voyagent dans les répertoires payload) :

```toml
[[package.metadata.shun.variant]]
name = "lite"                        # id + suffixe du nom d'artefact
payload = "payload-lite"             # remplacement du répertoire payload
faces = ["egui", "tui"]              # remplacements des faces pour le shell de la variante

[package.metadata.shun.variant.env]  # parvient au cargo build du shell
SHUN_FLAVOR = "lite"                 # (et son build.rs) — marqueurs de saveur
```

Appliquer une variante échange le répertoire payload et la liste des
faces du shell ; `env` voyage avec le build du shell pour que les
saveurs puissent y incruster des marqueurs. Un nom inconnu est une
erreur.

## Signature du code

`[package.metadata.shun.signing]` configure la signature des artefacts
construits — `shun build` signe l'installateur en fin de parcours et
`shun sign` signe des fichiers isolés (tous deux sautent avec
`--no-sign`). Absent = artefacts non signés, suffisant pour les tests
locaux :

```toml
[package.metadata.shun.signing.windows]   # Authenticode via signtool
enabled = true
thumbprint = "<empreinte du certificat>"  # choisit un certificat du magasin utilisateur/machine (/sha1)
# timestamp-url = "http://timestamp.digicert.com"   # RFC 3161 (/tr, la valeur par défaut)

[package.metadata.shun.signing.macos]     # codesign
enabled = true
identity = "<identité de signature>"
```

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

L'interface embarque dix locales (`en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`,
`fr`, `ru`, `es`, `de`, `pt`) avec leurs textes par défaut ; `shell.language = "auto"`
suit le système, une locale fixe l'épingle, et les licences par locale
restent localisées.

## Licence depuis le dépôt SySL

`[package.metadata.shun.license-sysl]` regroupe à la construction la
Synthetic Source License et ses traductions officielles depuis le dépôt
de la licence (requiert la feature `online`) — les assistants montrent
l'accord dans la langue de l'utilisateur sans vendoriser les fichiers
de licence :

```toml
[package.metadata.shun.license-sysl]
repo = "celestia-island/sysl"        # défaut
branch = "main"                      # défaut
locales = ["zh-Hans", "ja"]          # traductions à regrouper (l'anglais racine est toujours inclus)
```

Les étiquettes de locale se mappent sur les répertoires i18n du dépôt
sysl (`zh-Hans` → `zhs`, `zh-Hant` → `zht` ; `ja`/`ko`/`fr`/`ru`/`es`/
`de`/`pt`/`ar` en direct). Les documents regroupés s'affichent à
l'étape licence comme n'importe quel document de licence.

## Hooks de scripts

`[package.metadata.shun.script]` déclare des scripts de runner relatifs
au payload que le flux de livraison exécute aux frontières de phases.
Un runner par produit — seul `duckscript` (l'interpréteur embarqué) est
monté aujourd'hui :

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "post-install"               # prepare | post-install | pre-uninstall
script = "scripts/post_install.ds"   # relatif au payload, empaqueté avec lui
```

Un hook qui échoue fait échouer le flux. Le contrat complet — phases,
environnement vu par les hooks (`SHUN_LANGUAGE` entre autres) et
exemples — vit dans la [note de design sur le scripting](../design/scripting.md).
