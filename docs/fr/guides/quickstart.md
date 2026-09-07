# Démarrage rapide

shun se compose de deux moitiés : le **côté construction** (packaging du
payload, résolution du manifeste de livraison) et le **côté exécution** (un
shell qui pilote le flux).

## Essayer la démo

```bash
cargo run --example demo_flash                        # énumérer les périphériques flashables
cargo run --example demo_install                      # génère ShunDemo.shun + installation locale
cargo run --example demo_install -- --portable        # installation portable (sans registre)
cargo run --example demo_install -- --uninstall       # désinstallation (trace effacée)
```

`demo_install` génère le paquet d'installation `ShunDemo.shun`, le décompresse
avec une progression diffusée en continu et, en mode local, effectue
l'enregistrement façon NSIS : entrée ARP par utilisateur (Paramètres →
Applications), raccourci du menu Démarrer et désinstalleur auto-copiant. Le
mode portable n'écrit qu'un marqueur `.shun-portable` et ne touche jamais au
registre.

## Lancer le shell de démo

```bash
pnpm --dir shell/web install
cargo run -p shun_demo_shell
```

Le shell embarque le payload de démo à la construction (modèle
d'installateur mono-fichier) et rend les modes de livraison déclarés dans
`shell/Cargo.toml` → `[package.metadata.shun]`.
