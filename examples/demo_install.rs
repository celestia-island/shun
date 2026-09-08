//! CLI integration check over the comprehensive demo: loads the delivery
//! manifest declared by the demo application (`demo-app/Cargo.toml`),
//! generates the installer package (`<Product>.shun`), and drives the
//! install/uninstall flow against it with streamed progress events.
//!
//! ```text
//! cargo run --example demo_install                     # local install (ARP + uninstaller)
//! cargo run --example demo_install -- --portable       # portable install (no registry)
//! cargo run --example demo_install -- --uninstall      # remove a previous install
//! ```
//!
//! The full UI experience is the installer shell (`shell/`, `just demo`);
//! this example exercises the same flow headlessly. Local mode is
//! verifiable in Windows Settings → Apps; every trace is removed by the
//! uninstall pass.

use std::path::PathBuf;

use shun::config::{ShunConfig, TargetConfig};
use shun::flow::{Flow, FlowEvent};
use shun::payload::{ArchivePayload, pack_directory};
use shun::targets::install::{InstallContext, InstallFlow, WindowsRegistration, uninstall};

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    // 0. Resolve the demo application's delivery manifest — the same
    //    document the installer shell embeds (one source of truth).
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let demo_manifest = manifest_dir.join("demo-app").join("Cargo.toml");
    let config = ShunConfig::from_cargo_manifest(&demo_manifest)?;
    // Manifest payload paths are relative to the manifest's own directory.
    let payload_dir = demo_manifest
        .parent()
        .map(|dir| dir.join(config.payload.clone().unwrap_or_else(|| PathBuf::from("."))))
        .unwrap_or_else(|| manifest_dir.clone());

    let mut install_dir: Option<PathBuf> = None;
    let mut portable = false;
    let mut uninstall_mode = false;

    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--dir=") {
            install_dir = Some(PathBuf::from(value));
        } else {
            match arg.as_str() {
                "--portable" => portable = true,
                "--uninstall" => uninstall_mode = true,
                other => return Err(format!("unknown argument: {other}").into()),
            }
        }
    }

    // 1. Generate the demo installer package: the packed payload archive
    //    the shell (or this example) delivers.
    let product = config.product.name.clone();
    let package_path = std::env::current_dir()?.join(format!("{product}.shun"));
    let archive = pack_directory(&payload_dir)?;
    std::fs::write(&package_path, &archive)?;
    println!("✔ installer package written: {}", package_path.display());

    let install_dir = install_dir.unwrap_or_else(|| default_install_dir(&product, portable));
    let ctx = InstallContext {
        version: config.product.version.clone(),
        publisher: config.product.publisher.clone(),
        main_exe: config.targets.iter().find_map(|t| match t {
            TargetConfig::Install(install) => install.main_exe.clone(),
            _ => None,
        }),
        product,
        install_dir,
        portable,
        estimated_size_kb: 0,
    };

    // The entry executable only exists in the payload once the demo app
    // is staged (`just demo-payload`); local installs create a shortcut
    // to it, so an unstaged payload can only serve the portable check.
    let staged = ctx
        .main_exe
        .as_ref()
        .is_some_and(|main| payload_dir.join(main).is_file());
    if !staged {
        let main = ctx
            .main_exe
            .as_ref()
            .map(|main| main.display().to_string())
            .unwrap_or_else(|| "<unset>".into());
        if portable {
            println!(
                "△ demo application not staged — run `just demo-payload` for the full \
                 Tauri demo app before packing"
            );
        } else {
            return Err(format!(
                "demo application not staged — local installs create a Start-menu shortcut, \
                 which needs {main} in the payload; run `just demo-payload` first"
            )
            .into());
        }
    }

    if uninstall_mode {
        uninstall(&ctx, &WindowsRegistration)?;
        println!("✔ uninstalled {}", ctx.install_dir.display());
        return Ok(());
    }

    // 2. Drive the install flow; progress events stream from the real
    //    extraction work.
    let payload = ArchivePayload::from_bytes(&archive)?;
    let flow = InstallFlow {
        payload: &payload,
        registration: &WindowsRegistration,
        ctx: ctx.clone(),
    };
    flow.run(&mut print_event)?;

    let entry = ctx
        .main_exe
        .as_ref()
        .map(|main| ctx.install_dir.join(main))
        .unwrap_or_else(|| ctx.install_dir.clone());
    if ctx.portable {
        println!("✔ portable install complete: {}", ctx.install_dir.display());
        println!("  run it:            {}", entry.display());
        println!(
            "  remove it:         cargo run --example demo_install -- --uninstall --dir={}",
            ctx.install_dir.display()
        );
    } else {
        println!("✔ install complete: {}", ctx.install_dir.display());
        println!(
            "  verify it:         Settings → Apps → Installed apps → {}",
            ctx.product
        );
        println!(
            "  shortcut:          Start Menu → Programs → {}",
            ctx.product
        );
        println!("  remove it:         Settings → Apps, or rerun with --uninstall");
    }
    Ok(())
}

fn default_install_dir(product: &str, portable: bool) -> PathBuf {
    if portable {
        return std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(format!("{product}-portable"));
    }
    std::env::var("LOCALAPPDATA")
        .map(|local| PathBuf::from(local).join(product))
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(product))
}

fn print_event(event: FlowEvent) {
    match event {
        FlowEvent::Started => println!("▸ flow started"),
        FlowEvent::Progress {
            phase,
            step,
            percent,
        } => match percent {
            Some(percent) => println!("  [{percent:>3}%] [{phase:?}] {step}"),
            None => println!("  [ ··· ] [{phase:?}] {step}"),
        },
        FlowEvent::Completed => println!("✔ flow completed"),
        FlowEvent::Failed { message } => println!("✖ failed: {message}"),
        // FlowEvent is #[non_exhaustive] — future events stay forward-compatible.
        _ => {}
    }
}
