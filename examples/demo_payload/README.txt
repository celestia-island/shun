ShunDemo payload — the application directory delivered by the demo flow.

The committed files (this README, data/) travel with every demo install.
The application itself (bin/shun-demo.exe, a Tauri 2 demo app built from
demo-app/) is staged into bin/ by `just demo-payload` before the
installer shell embeds the payload — build products are never committed.

Delivery is declared in demo-app/Cargo.toml ([package.metadata.shun]);
this directory is only the payload root it points at.
