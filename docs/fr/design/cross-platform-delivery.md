# Note de conception : livraison multiplateforme

Statut : **surface Windows vérifiée et ses lacunes comblées (2026-09) ;
backends d'enregistrement à l'exécution Linux et macOS implémentés ;
mobile (Android/iOS) et HarmonyOS étudiés — hors périmètre pour
l'instant.** Cette note consigne (a) ce qu'un test concentré de la
surface d'enregistrement de l'empaquetage automatique a prouvé sur le
backend Windows livré, (b) les surfaces autrefois manquantes — raccourci
bureau / identité de barre des tâches / menu contextuel — désormais
implémentées, avec les constats de politiques de sécurité sur machine
réelle qui les ont façonnées, (c) les backends Linux et macOS, et (d) le
verdict de faisabilité pour Android, iOS et HarmonyOS (ajournés).

L'inventaire exécutable de (a) se trouve dans
`tests/registration_shortcuts.rs` et `tests/msix_pack.rs`.

## 1. Ce que le test Windows concentré a prouvé (2026-09, machine réelle)

Vérifié en exécutant le vrai `InstallFlow` sur une machine Windows 11 et
en relisant les résultats à travers Windows lui-même (résolveur COM
WScript.Shell, registre, MakeAppx du Windows SDK) — pas en inspectant
notre propre sortie :

| Surface | Résultat |
| --- | --- |
| Le `.lnk` du menu Démarrer se résout via le shell | **ok** — `TargetPath` et `WorkingDirectory` reviennent exactement tels que le contexte d'installation les a déclarés ; le PIDL synthétique construit à la main par `mslnk` se résout correctement |
| Chemins d'installation non ASCII (中文目录) | **ok** — un répertoire d'installation `顺测试目录` fait l'aller-retour par le résolveur COM octet pour octet |
| Binaire `.lnk` face à MS-SHLLINK | **ok** — en-tête, CLSID `{00021401-…}`, jeu d'indicateurs (target ID list + relative path + working dir + unicode), pas de raccourci clavier |
| Entrée ARP (HKCU) | **ok** — jeu de champs complet à la NSIS : DisplayName/Version/Publisher/InstallLocation/DisplayIcon plus `UninstallString`/`ModifyPath`/`RepairString` (tous entre guillemets, chacun invoquant l'interface du désinstalleur via `/uninstall`) et `EstimatedSize` en DWORD ; EstimatedSize correspond au total du manifeste du payload |
| Nettoyage à la désinstallation | **ok** — clé ARP, raccourci, payload, désinstalleur, répertoire : tout est retiré (couvert par `tests/install_local.rs`, reconfirmé ici) |
| Génération du manifeste MSIX | **ok** — identité, version à quatre parties complétée par des zéros, chaînes échappées pour XML, point d'entrée en barres obliques, runFullTrust |
| Empaquetage MSIX réel (MakeAppx 10.0.26100) | **ok** — zip OPC valide avec `[Content_Types].xml` + `AppxManifest.xml` ; le `dist/shundemo-0.1.0-x64.msix` produit ne porte **aucun bloc de signature** (par conception : la distribution via le Store le signe, ou bien un certificat auto-signé doit être approuvé) |

## 2. La passe de comblement des lacunes Windows (implémentée en 2026-09)

Tout ce qui suit a atterri après la passe de vérification, chaque
surface étant exercée par la suite de tests d'enregistrement sur une
machine réelle.

### Raccourci du bureau — `install.desktop-shortcut`

Résolu depuis la politique (`always` | `never` | `ask`, la convention de
la case à cocher NSIS — l'assistant egui montre un interrupteur coché
par défaut pour `ask`, les exécutions headless répondent « coché ») et
écrit à côté de celui du menu Démarrer. Le bureau est résolu via
**`SHGetKnownFolderPath(FOLDERID_Desktop)`** — jamais
`%USERPROFILE%\Desktop`, qui est faux dès que le bureau est redirigé
(OneDrive, stratégies de domaine). La désinstallation le retire
inconditionnellement (la configuration a pu changer entre
l'installation et la désinstallation).

**Mesuré sur machine réelle** : les politiques de sécurité (protections
faux-raccourci et rançongiciel des AV/EDR) refusent couramment la
création de `.lnk` sur le bureau *spécifiquement* — sur la machine de
vérification, même `echo x > Desktop\probe.lnk` depuis un shell élevé
est refusé alors que les fichiers `.tmp` s'écrivent librement. Le
raccourci bureau est donc **best-effort** : une écriture refusée est
rétrogradée en avertissement et ne fait jamais échouer l'installation
(le raccourci du menu Démarrer et l'entrée ARP sont la surface
critique). La suite de tests sonde la politique de la machine et
établit des assertions à la fois pour le chemin nominal et pour le
chemin de dégradation gracieuse.

### Identité de barre des tâches — `install.aumid`

L'épinglage programmatique à la barre des tâches reste **bloqué par
conception de la plateforme** (aucune API prise en charge ; les hacks
d'épinglage ont été retirés de Windows 10). Ce qui a été livré, c'est la
moitié identité : chaque raccourci est estampillé d'un
**`System.AppUserModel.ID`** à travers le magasin de propriétés COM du
Shell (`IShellLink` → `IPersistFile` → `IPropertyStore`, dans
`src/targets/aumid.rs` — `mslnk` n'écrit que des octets), pour que le
groupement en barre des tâches, les jump lists et l'épinglage *initié
par l'utilisateur* se comportent correctement. L'AUMID par défaut est
généré comme `{publisher}.{product}` ; `install.aumid` le surcharge, et
l'application devrait passer la même valeur à
`SetCurrentProcessExplicitAppUserModelID`. Les installations MSIX
obtiennent l'identité gratuitement via le paquet. L'estampillage est
best-effort pour la même raison de politique (la machine de
vérification refuse `IPropertyStore::SetValue` sur les fichiers `.lnk`
— 0x80030005 — l'estampille y est donc rétrogradée en avertissement).

### Verbes du menu contextuel — `[[install.verbs]]`

Niveau 1 livré : verbes de l'Explorateur par utilisateur sous
`HKCU\Software\Classes\Applications\<exe>\shell\<verb>\command` — la
surface documentée Application Registration, sans élévation, présents
sur l'exe de l'application et les raccourcis qui pointent vers lui.
Trois cibles de verbe se traduisent en lignes de commande sur chaque
plateforme qui les implémente : `data-folder` (ouvre le répertoire
d'installation), `uninstall` (exécute le désinstalleur copié), `app`
(point d'entrée + arguments). La désinstallation supprime les clés de
verbes qu'elle a créées, puis les conteneurs `shell`/`Applications`
seulement s'ils sont vides (un verbe enregistré par quelqu'un d'autre
survit). Le niveau 2 (associations de types de fichiers) et le niveau 3
(MSIX `FileExplorerExtension`) restent à venir.

### Liens profonds — `install.deep-links`

Le modèle de livraison les promet depuis la première ébauche ; ils sont
réels maintenant, sur chaque backend, par utilisateur : Windows
enregistre chaque schéma comme classe de protocole sous
`HKCU\Software\Classes\<scheme>` (le marqueur vide `URL Protocol` +
une commande d'ouverture qui reçoit l'URL en `%1`) ; Linux déclare
`MimeType=x-scheme-handler/<scheme>;` sur le lanceur et revendique le
gestionnaire par défaut via `xdg-mime` ; un `Info.plist` macOS synthétisé
porte `CFBundleURLTypes`. Les schémas se normalisent en minuscules
`[a-z0-9+.-]` (`"MyApp://"` → `myapp`). La désinstallation supprime la
classe de protocole Windows ; retirer le lanceur Linux rend orphelin le
gestionnaire (la ligne `mimeapps.list` devient inerte — constaté,
accepté).

### Pourquoi l'installeur ne demande pas d'élévation UAC

La question a surgi après les constats sur le raccourci bureau, et la
réponse tient en trois volets :

1. **Tout ce que shun écrit est de la surface par utilisateur** — HKCU,
   le menu Démarrer et le bureau de l'utilisateur, `%LOCALAPPDATA%`.
   Rien de tout cela n'a besoin d'un jeton élevé, donc une invite UAC
   n'apporterait rien tout en ajoutant le pire genre de bruit d'invites
   (habituer les utilisateurs à cliquer sans lire). C'est le même
   compromis que fait le modèle NSIS de wowsp, et le même que celui des
   installations *utilisateur* de VS Code / Chrome.
2. **L'élévation ne corrigerait de toute façon pas le blocage `.lnk`** :
   le bloc est un filtre de système de fichiers d'un produit de
   sécurité, indexé sur le dossier bureau et l'extension `.lnk` — les
   filtres interceptent des processus par politique, pas par ACL, et
   les processus élevés sont filtrés aussi. (Le cas « dossier contrôlé
   par Windows » se comporte pareil : il bloque même les administrateurs
   sauf si l'application est sur la liste d'autorisation.) La réponse
   correcte est celle livrée : dégrader en avertissement, garder le
   raccourci du menu Démarrer et l'entrée ARP intacts.
3. **Les écritures élevées vers des surfaces *utilisateur* sont un piège
   d'exactitude** : un processus élevé résout les profils différemment
   (le bureau, `%APPDATA%` et la ruche de registre d'un compte admin
   peuvent tous différer de ceux de l'utilisateur qui installe) — le bug
   classique des raccourcis tous-utilisateurs de NSIS. Quand
   l'élévation est réellement nécessaire, l'étape concernée s'élève
   *elle-même* : le bootstrapper Evergreen de WebView2 transporte son
   propre manifeste `requireAdministrator`, donc le shell reste
   `asInvoker` et délègue.

Une portée à l'échelle de la machine (`Program Files`, ARP HKLM,
raccourcis tous-utilisateurs) est un *mode* légitime dont certains
produits ont besoin — il a été livré exactement ainsi : un opt-in
délibéré (`install.scope`), jamais le défaut. Voir la section 6.

### Constats de robustesse, les deux corrigés

- Les noms de produits comportant des caractères illégaux pour des noms
  de fichiers (`/\:*?"<>|`, points/espaces finaux) sont **assainis sur
  leur radical** pour toute surface de système de fichiers et de
  registre (noms `.lnk`, chemin de clé ARP) — un `\` dans un nom de
  produit ne crée plus de sous-clés de registre imbriquées.
- Le `UninstallString` ARP passe `/uninstall`, mais l'analyseur headless
  du shell d'installation n'acceptait que `--uninstall` — cliquer
  « Désinstaller » dans les Paramètres Windows lançait l'assistant au
  lieu de désinstaller. Les deux orthographes sont maintenant analysées.

## 3. Linux et macOS (backends d'exécution implémentés ; artefacts de packaging ensuite)

**Les deux sont abordables, et les deux partagent avec Windows une même
limite dure : l'épinglage programmatique barre des tâches/dock n'existe
nulle part.** Les backends `Registration` à l'exécution sont livrés ;
les artefacts de packaging côté construction (deb/rpm via
`tauri-bundler`, DMG) restent des travaux de suivi car ils exigent leurs
hôtes de construction natifs.

### Linux — `LinuxRegistration` (src/targets/freedesktop.rs)

Tout ce que fait le backend Windows se mappe sur les conventions
freedesktop, tout par utilisateur (`~/.local/share/...`), sans
élévation :

- **enregistrement du lanceur** = écrire `<product>.desktop` (Name,
  Exec, Icon depuis `install.icon`, Categories, et — crucial —
  **`StartupWMClass`** = le radical de l'exécutable d'entrée, le champ
  qui fait qu'un épinglage barre des tâches/dock initié par
  l'utilisateur se groupe sous la bonne icône) dans
  `~/.local/share/applications`, puis y exécuter
  `update-desktop-database` (le sauter est sans risque quand l'outil est
  absent — les environnements de bureau rescannent paresseusement) ;
- **verbes du menu contextuel + entrée de désinstallation** = `Actions=`
  + groupes `[Desktop Action <id>]` — incluant toujours une action
  **Uninstall**, parce que GNOME Software / KDE Discover ne listent que
  les applications suivies par leurs propres backends de paquets : une
  application installée par shun n'y apparaît jamais. Les trois cibles
  de verbe se mappent vers `xdg-open`, le désinstalleur, et le point
  d'entrée + arguments ;
- **restauration du bit d'exécution** — l'archive du payload transporte
  0644 pour chaque entrée, le backend repasse donc en chmod 0755 le
  point d'entrée et le désinstalleur copié ;
- **désenregistrement** = supprimer le `.desktop` + rafraîchir la base
  de données.

L'auteur du `.desktop` n'est que de la plomberie de données, qui compile
(et est testée unitairement) sur toutes les plateformes ; seule la
moitié qui lance des processus est conditionnée à Linux. L'**épinglage**
barre des tâches/dock **reste impossible** (aucune API
inter-environnements : les favoris GNOME sont une clé gsettings interne,
les épingles KDE vivent dans un appletsrc non documenté ; à considérer
comme une action utilisateur). Les menus contextuels des gestionnaires
de fichiers (scripts Nautilus / menus de service Dolphin) restent hors
périmètre.

**Formats de packaging** (toujours côté construction, en suivi) :
**tarball/portable (shun l'a déjà) + deb ([cargo-deb]) + rpm
([cargo-generate-rpm])** est le meilleur sous-ensemble — exactement ce
qu'émet `tauri-bundler` (c'est une bibliothèque utilisable pour des
payloads non-Tauri, tout comme [cargo-packager]). AppImage = moyen ;
Flatpak = moyen-haut ; snap = haut, à différer. La limite honnête du
« tourne sur toutes les distributions » : glibc n'est compatible que
vers l'avant et **les binaires statiques musl ne peuvent pas porter
d'applications WebView** (webkit2gtk traîne toute la pile C de GTK) —
le shell de livraison peut être musl-statique, mais les applications
Tauri livrées doivent être construites contre la plus ancienne baseline
webkit2gtk-4.1 prise en charge (ère Ubuntu 22.04 / Debian 12 / Fedora
37+).

### macOS — `MacOSRegistration` (src/targets/macos.rs)

- **enregistrement** = localiser le bundle `.app` où vit l'exécutable
  d'entrée (l'ancêtre `.app` le plus proche) ; synthétiser un
  `Info.plist` minimal (`plist.rs`, pur et testé unitairement partout)
  quand le payload n'en transportait pas ; restaurer le bit d'exécution
  du point d'entrée ; **retirer récursivement le `com.apple.quarantine`
  hérité** de l'installation (les navigateurs estampillent l'installeur,
  et les copies macOS préservent les xattrs — sans cela, l'application
  livrée hérite du blocage Gatekeeper auquel l'utilisateur avait déjà
  répondu) ; puis `lsregister -f` sur le bundle — Spotlight et Launchpad
  suivent. Le désenregistrement = `lsregister -u` (les fichiers sont
  retirés par la passe de désinstallation générique). Les payloads
  exécutable seul (sans `.app`) s'enregistrent en no-op — conventions
  portables ;
- **épinglage au Dock** : **aucune API prise en charge** (le hack
  `defaults write com.apple.dock` + `killall Dock` écrase les préférences
  utilisateur et n'est pas fiable sur les macOS récents) — la présence
  Launchpad/Spotlight via l'enregistrement LS en est l'équivalent côté
  découvrabilité ;
- **la signature reste un travail de processus obligatoire** pour une
  vraie distribution : Developer ID + runtime endurci + `notarytool` +
  staple ; et le shell téléchargé **sera transloqué** — traitez les
  hypothèses sur le chemin propre en conséquence ;
- **artefacts de packaging** (en suivi) : DMG via tauri-bundler /
  `hdiutil`, `.pkg` seulement pour les parcours administrateur, cask
  Homebrew comme canal. Livrez en **universal2** (double build +
  `lipo` + re-signature).

[cargo-deb]: https://github.com/kornelski/cargo-deb
[cargo-generate-rpm]: https://crates.io/crates/cargo-generate-rpm
[cargo-packager]: https://github.com/crabnebula-dev/cargo-packager

## 4. Android et iOS (étudiés)

**Reformulons d'abord : sur mobile, l'« installation » appartient à la
plateforme et est vérifiée par signature. Il n'existe pas d'équivalent
du streaming d'un tar zstd d'un répertoire applicatif vers un
emplacement choisi par l'utilisateur.** Le rôle mobile honnête de shun
est un **pipeline de construction/packaging/signature** (un « cargo-dist
pour le mobile »), pas un runtime d'installation.

### Android

- **Mécanique de packaging** : APK = zip signé (DEX + ressources +
  `.so` par ABI) ; pipeline `aapt2` → `d8`/`r8` → package →
  `zipalign` → `apksigner`. L'AAB est le format de publication Play
  (requis pour les nouvelles applications) ; le sideloading exige un
  APK concret (`bundletool build-apks` + re-signature). Les cibles Rust
  sont Tier 2 avec outils hôte.
- **L'outillage vivant (2025–2026)** : le `tauri android build` de
  Tauri 2 (enveloppe cargo-mobile2 + Gradle ; keystore via
  `keystore.properties`), **cargo-ndk** (maintenu), la crate **`apk`**
  (aapt2/d8/zipalign/apksigner sans Gradle). **xbuild et cargo-apk sont
  morts/dormants** — ne bâtissez pas dessus. Tauri 2 mobile est
  officiellement stable ; le côté Android est plus mûr que l'iOS.
- **Installeur à l'exécution : infaisable/sans objet.** Le gestionnaire
  de paquets de l'OS installe à partir d'un APK signé avec une
  confirmation côté utilisateur (silencieux uniquement pour
  device-owner/MDM) ; l'APK *est* l'installeur. La politique Play
  **interdit de surcroît l'auto-mise à jour / le téléchargement de code
  exécutable** — un runtime de mise à jour façon shun serait une
  violation de politique pour toute application distribuée via Play.
  Le sideloading se dégrade par conception : **l'application du
  sideloading par développeur vérifié commence le 2026-09-30** (les
  applications non vérifiées tombent sur un parcours en plusieurs
  étapes avec une attente de 24 heures).
- **Raccourcis** : icônes d'écran d'accueil et raccourcis d'application
  (`shortcuts.xml`, `ShortcutManager`, `requestPinShortcut`) sont
  **déclarés par l'application uniquement** — aucune API d'injection au
  moment de l'installation n'existe. L'analogue au moment de la
  construction est la génération de `shortcuts.xml`/intent-filters dans
  l'APK que shun empaquette.

### iOS

- **Mécanique de packaging** : IPA = zip avec `Payload/App.app` +
  `embedded.mobileprovision` (App ID + entitlements + certificat +
  allowlist UDID Ad Hoc). Canaux : App Store, TestFlight, Ad Hoc
  (100 appareils/type/an), Enterprise. **La chaîne d'outils de signature
  (codesign, xcodebuild, keychain) est macOS uniquement** — exigence
  matérielle d'hôte.
- **Installeur à l'exécution : infaisable** hors deux niches : (a) les
  manifestes OTA Ad Hoc (`itms-services://?...manifest.plist`) — facile,
  légal, niche ; (b) le régime UE Web Distribution / marchés
  alternatifs — réel mais conditionné par l'éligibilité Apple +
  notarisation + les conditions tarifaires d'octobre 2026 (commission
  Core Technology de 5 %), et limité à l'UE. Le sideloading à Apple ID
  gratuit (AltStore/Sideloadly) est un chemin d'amateur à 7 jours/3
  applications, pas un canal productisable.
- **Raccourcis** : rien n'existe au moment de l'installation, dans aucun
  canal — icônes d'écran d'accueil, schémas d'URL, universal links sont
  déclarés par l'application et validés par signature.
- **Repli egui** : Android = `android-activity` + winit + wgpu
  (Vulkan/GLES) ; iOS = winit + wgpu (Metal) embarqués dans un hôte
  UIKit via FFI + projet Xcode. Aucun des deux n'a de solution clés en
  main — un pipeline de packaging shun est précisément le morceau
  manquant.

## 5. HarmonyOS (étudié)

**Verdict d'emblée : « shun prend en charge HarmonyOS » ne peut
honnêtement signifier qu'une seule chose en 2026 — une cible de
packaging/signature au moment de la construction produisant des
artefacts HAP/APP signés pour publication, plus un parcours `hdc
install` pour appareils de développement. Un installeur à l'exécution
(la moitié NSIS de shun) n'a aucun substrat légal ni technique sur
HarmonyOS NEXT.**

Paysage (2025–2026) : HarmonyOS NEXT (5.0, oct 2024) a abandonné la
couche de compatibilité APK ; la ligne à viser est HarmonyOS 6+ (API
20/23), Chine uniquement, AppGallery uniquement, ~19 % du marché OS
chinois. OpenHarmony est la base ouverte ; le HarmonyOS commercial est
le produit de Huawei par-dessus — un empaqueteur vise le commercial.

- **Format de paquet** : HAP (zip : `module.json5`, bytecode ArkTS,
  `libs/<abi>/*.so` natifs) ; paquets partagés HSP/HAR ; `.app` = pack
  de soumission AppGallery (`pack.info`). L'outillage est utilisable en
  CLI : `ohpm` + `hvigorw assembleHap` + `hap-sign-tool` +
  `app_packing_tool.jar` (builds CI headless officiellement pris en
  charge).
- **Signature** : SHA256withECDSA ; keystore `.p12` + certificat
  `.cer` + profil `.p7b` (nom de bundle, permissions, et pour le debug
  l'allowlist UDID des appareils) ; les certificats sont **émis par
  Huawei via AppGallery Connect** (inscription individuelle gratuite —
  pas de frais à la Apple).
- **Rust** : `aarch64/armv7/x86_64-unknown-linux-ohos` sont **Tier 2
  avec outils hôte** (prêts via rustup depuis la 1.78). La chaîne
  communautaire `ohos.rs` (`cargo-ohos`, `napi-ohos`, `ohos-openssl`)
  fait la colle ; noyau Rust + coquille ArkTS est une architecture
  éprouvée (RustDesk OHOS). **egui est bloqué** : winit n'a pas de
  backend OHOS amont (bêtas communautaires seulement). **Tauri** : une
  branche officielle-mais-non-fusionnée `feat/open-harmony` (patchs
  wry/tao, CLI `cargo tauri ohos`) fonctionne aujourd'hui mais bouge
  vite — attendez-vous à re-épingler tous les quelques mois.
- **Honnêteté sur la distribution** : le sideloading grand public est
  effectivement fermé (AppGallery uniquement ; `hdc install` exige le
  mode développeur + signature Huawei + UDID). Publication sur
  appareils désignés : 100 appareils/an, validité de 90 jours. La
  distribution entreprise est pour l'instant cantonnée aux PC
  d'entreprise Qingyun. **HarmonyOS PC** est réel (ARM, distribution
  par le store, pas encore de sideloading) — Huawei a *affiché
  l'intention* d'ouvrir le sideloading PC plus tard ; c'est le seul
  point de veille qui pourrait un jour y justifier un runtime de
  livraison desktop.

Table d'effort : cible de packaging HAP **moyenne** ; étape de
cross-compilation Rust **facile–moyenne** (wrapper clang du SDK comme
éditeur de liens, ohos-openssl pour TLS) ; packaging
Tauri-sur-OHOS **moyen–difficile** (non fusionné en amont) ; repli egui
**difficile** (pas de winit) ; installeur/flasheur à l'exécution
**infaisable**.

## 6. Portée d'installation et assistant déclaratif (implémentés en 2026-09)

### Portée d'installation — `install.scope = user | machine | ask`

Le par utilisateur reste le défaut (voir « Pourquoi l'installeur ne
demande pas d'élévation UAC »). `machine` — ou un `ask` répondu « tous
les utilisateurs » — fait basculer chaque surface d'enregistrement vers
son équivalent à l'échelle de la machine : l'entrée ARP sous **HKLM**,
le raccourci dans le **menu Démarrer tous utilisateurs**
(`%ProgramData%`), le raccourci bureau sur le **bureau public**
(`FOLDERID_PublicDesktop`), verbes et liens profonds sous
`HKLM\Software\Classes`. Le shell détecte la résolution **avant** que
le flux ne s'exécute et, s'il n'est pas encore élevé, se relance
lui-même via le verbe `runas` en portant les réponses de l'utilisateur
(`--silent --mode=… --dir=… --scope=machine`) : le consentement UAC est
l'unique invite, affichée seulement pour le mode qui en a besoin — le
motif du bootstrapper, exactement comme promis. La désinstallation fait
miroir (le `UninstallString` ARP lance le désinstalleur, qui s'élève de
la même façon). La portée machine est Windows uniquement ; les backends
Linux/macOS la refusent explicitement. Le test d'intégration saute sauf
si le runner est élevé (un `cargo` depuis un shell administrateur
l'exerce).

### Le pipeline de l'assistant — `[[package.metadata.shun.steps]]`

L'assistant est désormais un pipeline déclaratif, ordonné et librement
composé, au lieu d'une séquence fixe mode → installation. Cinq types
d'étapes :

```toml
[[package.metadata.shun.steps]]
kind = "mode"                    # mode de livraison + répertoire ; embarque
                                 # les interrupteurs `ask` (raccourci
                                 # bureau, portée)
[[package.metadata.shun.steps]]
kind = "scope"                   # choix autonome utilisateur/machine
[[package.metadata.shun.steps]]
kind = "license"                 # volet de licence (license / license-locales)
[[package.metadata.shun.steps]]
kind = "content"                 # volet markdown personnalisé
title = "Release notes"
markdown = "notes.md"         # relatif au manifeste
[[package.metadata.shun.steps]]
kind = "install"                 # l'exécution de livraison (exactement une requise)
```

Sans `steps` = le pipeline par défaut (mode → licence-si-déclarée →
install) avec les `custom-steps` historiques injectés après leurs clés
`after` ; déclarer les deux est une erreur de configuration, comme zéro
ou plusieurs étapes `install`. La question de portée et l'interrupteur
du raccourci bureau apparaissent partout où les politiques `ask`
rencontrent un volet : embarqués dans l'étape mode, ou autonomes
(`scope`) — c'est le développeur qui compose. Les documents de contenu
et de licence sont lus **au moment de la construction** et inlinés dans
`shun-steps.json` (`ShunConfig::resolve_steps`), de sorte que les
installeurs à l'exécution n'emportent aucune dépendance de fichiers.
Le repli egui rend le pipeline complet (rail par étape, blocage par
licence, navigation retour/suivant) ; le `ShellView` Tauri expose les
étapes résolues au front-end web.

## 7. État du portefeuille (2026-09)

| Capacité | Win | Linux | macOS | Android | iOS | HarmonyOS |
| --- | --- | --- | --- | --- | --- | --- |
| Installation + enregistrement à l'exécution | ✅ portée utilisateur + machine | ✅ backend `.desktop` (utilisateur) | ✅ backend `.app` (utilisateur) | ajourné | ajourné | ajourné |
| Raccourci bureau/menu Démarrer | ✅ les deux (selon politique) | ✅ entrée de lanceur | ✅ (Launchpad/Spotlight) | sans objet | sans objet | sans objet |
| Épinglage barre des tâches/dock | identité seule (estampille AUMID) | identité seule (StartupWMClass) | identité seule (LS) | sans objet | sans objet | sans objet |
| Menu clic droit | ✅ verbes de niveau 1 | ✅ Desktop Actions | NSServices plus tard | ajourné | ajourné | ajourné |
| Liens profonds | ✅ classe de protocole | ✅ MimeType + xdg-mime | ✅ CFBundleURLTypes | ajourné | ajourné | ajourné |
| Scénario de désinstallation | ✅ ARP + auto-suppression | ✅ à base d'Actions | ✅ désenregistrement LS + fichiers | OS | OS | OS |
| Artefacts de packaging | ✅ installeur + MSIX | tarball ✅ ; deb/rpm ensuite | bundle ✅ ; DMG ensuite | ajourné | ajourné | ajourné |
| Réalité de la signature | Authenticode / Store | optionnelle | obligatoire (travail de processus) | keystore / Play | certificats Apple | Huawei AGC |

**Ajournés par décision (2026-09)** : Android, iOS et HarmonyOS — les
recherches des sections 4–5 restent valables ; rien de tout cela n'est
planifié.

**Ensuite dans la file** :

1. Artefacts de packaging deb/rpm via `tauri-bundler` (CI Linux), DMG
   via la voie hôte macOS.
2. Niveau 2 du menu contextuel Windows (associations de types de
   fichiers), si un consommateur en a besoin.
3. **Jamais** : runtimes d'installation / injection de raccourcis sur un
   OS téléphonique quelconque ; épinglage barre des tâches/dock où que
   ce soit ; snap tant qu'il n'a pas mérité sa complexité.
