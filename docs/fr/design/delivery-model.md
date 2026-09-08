# Modèle de livraison

shun prend en charge la moitié « livraison » de la publication de logiciels
de bureau. Trois pièces orthogonales :

## Payload

Un répertoire applicatif empaqueté en tar compressé zstd avec un manifeste
SHA-256 (`shun-manifest.json`). L'archive est embarquée dans le binaire de
l'installeur (`include_bytes!`, installeur mono-fichier) ou transportée en
sidecar. L'extraction vérifie chaque entrée contre le manifeste et diffuse
les événements de progression.

## Flow

Une exécution de livraison est une suite de `FlowEvent` — `started`,
`progress { phase, step, percent }`, `completed`, `failed` — rendus
directement par l'interface du shell. La progression est **multi-couches** :
un installeur en ligne fait avancer simultanément les phases de
téléchargement (download) et d'extraction (extract). Le flux d'installation
extrait le payload, écrit le manifeste sur disque (consommé par la
désinstallation), puis enregistre (mode local) ou pose le marqueur portable
(mode portable).

## Targets

- **install** — enregistrement direct par plateforme : sous Windows, une entrée ARP par utilisateur, un désinstalleur auto-copié, des raccourcis menu Démarrer/bureau (avec AUMID) et des verbes optionnels du menu contextuel de l'Explorateur ; sous Linux, un lanceur `.desktop` par utilisateur avec des actions de bureau (dont Désinstaller) ; sous macOS, complétion du bundle `.app` plus enregistrement Launch Services. Le mode portable ne touche aucun état système, sur aucune plateforme. La désinstallation supprime toute trace selon le manifeste.
- **flash** — écriture sur périphérique bloc et vérification après écriture
  (flash d'images). Le backend arrive avec le flasheur evernight ; la surface
  trait et l'énumération des périphériques sont disponibles dès aujourd'hui.

## WebView2 (Windows)

Le shell est lui-même une application Tauri : le runtime WebView2 est un
prérequis strict de sa propre interface. Le manifeste de livraison choisit la
stratégie : exiger le runtime système, embarquer l'installeur hors ligne
Evergreen, ou transporter en privé un runtime à version fixe — une copie
partagée par le shell et l'application installée, à travers les modes install
et portable.
