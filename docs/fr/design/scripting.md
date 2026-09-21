# Note de conception : script à l'installation

Statut : **runner décidé — duckscript. Python étudié et embarquabilité
prouvée ; adoption en attente.** duckscript est l'unique runner de
scripting ; l'option JavaScript est abandonnée (l'outillage cargo-make
autour de duckscript est complet, et là où il ne l'est pas, appeler
Python sert d'échappatoire). Cette note consigne la décision et la
recherche sur l'embarquement de Python.

## duckscript (le runner)

[`duckscriptsdk`](https://crates.io/crates/duckscriptsdk) (Apache-2.0,
le langage de scripting de cargo-make) s'embarque comme une dépendance
ordinaire : charger le jeu de commandes dans un `Context`, enregistrer
les built-ins de shun comme commandes personnalisées, exécuter des
scripts avec contrôle de flux et std fs/env/net. La preuve de
faisabilité se trouve dans `tests/scripting_duckscript.rs`. justfile
lui-même n'est pas embarquable (la crate `just` est une CLI, sans API
de bibliothèque stable) — duckscript est le membre embarquable de cette
famille.

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "prepare"                 # prepare | post-install | pre-uninstall
script = "installer/prepare.dk"   # empaqueté par `shun build`
```

La surface des built-ins shun s'enregistre en commandes duckscript :
`shun_progress`, `shun_emit`, `shun_fetch` (téléchargements vérifiés),
plus les propres commandes std du SDK (fs, env, http, process, semver,
...).

Quand les hooks de scripts tourneront, le flux d'installation
exportera la langue de l'assistant à chaque étape de script comme
`SHUN_LANGUAGE` (la langue choisie à la première étape de l'assistant,
consignée aussi dans le manifeste d'installation sur disque). La
variable est absente quand aucune langue n'a été choisie. Exporter
l'information est tout ce que fait shun — écrire la langue dans la
configuration propre à l'application installée relève du script du
payload.

Pièges à normaliser dans les wrappers shun : les chemins Windows à
antislashs sont des caractères d'échappement dans les arguments
duckscript (passez des chemins à barres obliques), et l'affectation est
la syntaxe de capture de sortie (`x = cmd args`).

## Python embarqué — mesuré (sonde exécutée en 2026-09)

Tout ce qui suit a réellement tourné, deux fois : sur un CPython 3.13.5
hôte et sur un **runtime embarquable transporté**
(`python-3.13.5-embed-amd64.zip` dépaqueté, avec l'exemple PyO3
`pyembed_runner` placé à côté, pour que `python313.dll`/stdlib se
chargent depuis le dossier transporté — `sys.prefix` a confirmé le
dossier transporté) :

| Capacité | Résultat |
| --- | --- |
| Vrai HTTPS (urllib + TLS) | ok (pypi.org en direct était bloqué par le réseau localement ; example.com/miroir tencent ok) |
| SHA-256 + HMAC en streaming | ok |
| AES-CTR aller-retour, signature/vérification RSA-2048 | ok — via la wheel `cryptography` préinstallée dans le runtime transporté (`pip --target runtime/Lib/site-packages` + activation de `import site` dans `python313._pth`) |
| Identité machine | MachineGuid (winreg), MAC (`uuid.getnode`), numéro de série du volume C: (ctypes `GetVolumeInformationW`) — tout ok |
| TPM | `tbs.dll` atteinte correctement via ctypes ; le firmware de la machine de sonde a le TPM désactivé, donc `Tbsi_Context_Create` retourne `TBS_E_TPM_NOT_FOUND` (0x8028400F — note : PAS 0x80284002, qui est `TBS_E_BAD_PARAMETER` venant d'une struct de paramètres NULL). Le chemin d'appel est validé ; sur du matériel avec TPM activé, le même code lit `TPM_PT_MANUFACTURER` |

Tailles mesurées : zip embarquable **10,9 Mo** / dépaqueté **20,4 Mo** /
+wheel cryptography **32,4 Mo**. Le binaire runner PyO3 fait lui-même
~0,2 Mo. Les wheels tierces à `.pyd` natifs (comme cryptography)
fonctionnent inchangées — livrez-les à l'intérieur du runtime
transporté.

Pièges consignés pour la vraie intégration : l'interpréteur embarqué ne
se finalise pas au drop — flushez stdio explicitement après l'exécution
des scripts (voir l'exemple du runner) ; `eval` n'accepte que des
expressions ; pip contre le runtime transporté exige `--target` plus
l'ajustement du `._pth` (ou un runtime python-build-standalone, qui
embarque pip).

## Embarquement du WebView2 à version fixe — mesuré

Question : l'installeur peut-il transporter lui-même le moteur WebView2,
alimentant à la fois sa propre interface et l'application déployée ?
**Mécaniquement oui — prouvé de bout en bout** ; le coût, c'est le
payload.

- cab à version fixe v151.0.4129.101 x64 : **307 241 094 octets ≈
  293 Mo** compressés, **661,1 Mo** dépaquetés.
- Le shell de démo a tourné contre le runtime transporté dépaqueté
  (`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`, déjà la première sonde de
  `webview2_available`) : interface rendue (capture hors ligne
  vérifiée) et **les six processus renderer venaient tous du dossier
  transporté**, pas de l'installation Evergreen système.
- Verdict : faisable ; le coût de ~300 Mo est **accepté** (en ligne avec
  les autres empaqueteurs). L'inquiétude de la double copie est
  éliminée par conception : l'artefact embarque UNE copie — le shell
  s'amorce depuis le sous-arbre runtime du payload (staging
  `extract_prefix` + réutilisation sensible au hash à l'extraction),
  donc l'installeur et l'application installée le partagent ; voir la
  section stratégies WebView2 dans configuration.md. Le repli egui
  reste le plancher à coût zéro pour les machines qui n'ont rien du
  tout.

## Python embarqué (étudié, faisable)

**Verdict : oui — Rust peut embarquer un petit CPython, proprement.**
La preuve est `tests/scripting_python.rs` derrière la fonctionnalité
opt-in `python-probe` : [PyO3](https://pyo3.rs) avec `auto-initialize`
embarque l'interpréteur dans le processus, évalue du vrai Python avec
la stdlib, appelle des fonctions Rust personnalisées et mappe les
exceptions Python en erreurs Rust. La fonctionnalité n'est jamais dans
le build par défaut ; sur CI, seule la jambe Windows (qui résout
`--all-features` contre un CPython préinstallé) l'exerce.

Options de livraison pour un installeur autonome, en miroir du tableau
des stratégies WebView2 :

| Option | Transporte | Notes |
| --- | --- | --- |
| `system` (défaut) | rien | les commandes `process` de duckscript peuvent invoquer un python installé ; se dégrade élégamment en son absence |
| `embeddable` | paquet embarquable Windows (~12–16 Mo) | `python-3.x.x-embed-amd64.zip` officiel : `python3xx.dll` + zip stdlib + `._pth`, sans admin, sans registre — un runtime privé exactement comme le WebView2 à version fixe |
| `standalone` | python-build-standalone (~30–60 Mo) | distributions [supervisées par Astral](https://astral.sh/blog/python-build-standalone) (ce que `uv` livre) ; multiplateformes, épinglées, complètes ; excessif sauf si pip/dépendances natives sont nécessaires |

Esquisse :

```toml
[package.metadata.shun.script.python]    # échappatoire lourde optionnelle
type = "embeddable"                      # system | embeddable | standalone
```

Alternatives rejetées/ajournées :

- **RustPython** (MIT, Rust pur) — autodéclaré non prêt pour la
  production, lacunes de stdlib, pas de modules d'extension C ;
  attirant un jour, pas pour des installeurs aujourd'hui.
- **PyOxidizer / `pyembed`** — embarquement de plus haut niveau, mais
  le projet est en mode maintenance ; PyO3 seul suffit ici.

Questions ouvertes avant de le brancher :

- Budget de taille : +12–16 Mo sur l'artefact, est-ce acceptable pour
  les produits qui l'activent ? (L'embarquement est opt-in par
  manifeste, donc l'artefact par défaut reste petit.)
- Couplage de versions : PyO3 lie le CPython de l'hôte de build ; le
  runtime livré doit correspondre. Épinglez en construisant contre la
  distribution même que nous livrons (`PYO3_PYTHON` → dossier
  embeddable/standalone dépaqueté).
- Isolation : interpréteur embarqué in-process (UX d'installeur
  mono-fichier) vs sous-processus (isolation de crash plus simple) — ou
  les deux, au choix par hook.
- Quels hooks peuvent recourir à Python en premier lieu (prepare
  seulement, ou aussi les parcours de réparation post-install ?).
