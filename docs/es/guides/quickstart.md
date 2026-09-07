# Inicio rápido

shun se compone de dos mitades: el **lado de construcción** (empaquetar el
payload, resolver el manifiesto de entrega) y el **lado de ejecución** (un
shell que pilota el flujo).

## Prueba la demo

```bash
cargo run --example demo_flash                        # enumerar dispositivos grabables
cargo run --example demo_install                      # genera ShunDemo.shun + instalación local
cargo run --example demo_install -- --portable        # instalación portable (sin registro)
cargo run --example demo_install -- --uninstall       # desinstalación (borra todo rastro)
```

`demo_install` genera el paquete de instalación `ShunDemo.shun`, lo extrae
con progreso en streaming y — en modo local — realiza el registro estilo
NSIS: entrada ARP por usuario (Configuración → Aplicaciones), acceso directo
en el menú Inicio y un desinstalador que se auto-copia. El modo portable solo
escribe un marcador `.shun-portable` y nunca toca el registro.

## Ejecutar el shell de demo

```bash
pnpm --dir shell/web install
cargo run -p shun_demo_shell
```

El shell incrusta el payload de demo en tiempo de construcción (patrón de
instalador mono-archivo) y muestra los modos de entrega declarados en
`shell/Cargo.toml` → `[package.metadata.shun]`.
