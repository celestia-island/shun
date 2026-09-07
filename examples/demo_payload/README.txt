ShunDemo payload — a stand-in "application" used by the demo examples.

This directory is packed verbatim by `cargo run --example demo_install`
into `ShunDemo.shun` (a zstd tar with a SHA-256 manifest) and delivered by
the install flow into the chosen target directory.
