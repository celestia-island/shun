mod common;

#[cfg(windows)]
use common::{ctx, demo_payload};
use shun::flow::Flow;
#[cfg(windows)]
use shun::targets::install::{InstallFlow, WindowsRegistration, arp_key_path};

/// Local-mode install performs real HKCU ARP registration; the uninstall
/// pass removes every trace. Windows-only (the backend is windows).
#[cfg(windows)]
#[test]
fn local_install_registers_arp_and_uninstall_cleans() {
    use shun::targets::install::uninstall;

    let dest = tempfile::tempdir().unwrap();
    let install_dir = dest.path().join("ShunDemo");
    let product = "ShunDemo-Test-Local";
    let context = ctx(product, install_dir.clone(), false);

    let payload = demo_payload();
    let flow = InstallFlow {
        payload: &payload,
        registration: &WindowsRegistration,
        ctx: context.clone(),
    };
    flow.run(&mut |_| {}).unwrap();

    // ARP entry exists with an UninstallString pointing at the copied
    // uninstaller.
    let key = arp_key_path(product);
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let arp = hkcu
        .open_subkey_with_flags(&key, winreg::enums::KEY_READ)
        .unwrap();
    let uninstall_string: String = arp.get_value("UninstallString").unwrap();
    assert!(uninstall_string.contains("uninstall.exe"));
    assert!(uninstall_string.contains("/uninstall"));
    assert!(install_dir.join("uninstall.exe").exists());

    // Start-menu shortcut exists.
    let link = std::env::var("APPDATA")
        .map(|appdata| {
            std::path::Path::new(&appdata)
                .join(r"Microsoft\Windows\Start Menu\Programs")
                .join(format!("{product}.lnk"))
        })
        .unwrap();
    assert!(link.exists());

    // Uninstall removes the ARP key, shortcut, uninstaller, payload files,
    // and the directory itself.
    uninstall(&context, &WindowsRegistration).unwrap();
    assert!(
        hkcu.open_subkey_with_flags(&key, winreg::enums::KEY_READ)
            .is_err()
    );
    assert!(!link.exists());
    assert!(!install_dir.join("uninstall.exe").exists());
    assert!(!install_dir.exists());
}
