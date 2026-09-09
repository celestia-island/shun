//! Focused verification of the auto-packaging registration surface on
//! Windows — what the install flow actually injects today, verified the
//! way Windows itself sees it:
//!
//! - the Start-menu `.lnk` is parsed back through the WScript.Shell COM
//!   resolver (not just checked for existence), because `mslnk` builds a
//!   synthetic PIDL by hand — if the shell cannot resolve it, the
//!   shortcut is broken no matter how well-formed the bytes look;
//! - the ARP entry carries the full NSIS-equivalent field set;
//! - the deliberate *absences* are pinned: no desktop shortcut, no
//!   taskbar pin, no Explorer verb is written today. These assertions
//!   are the executable inventory for the desktop-shortcut / taskbar /
//!   context-menu work — flip them when that support lands.

mod common;

#[cfg(windows)]
mod registration {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use crate::common::{ctx, demo_payload};
    use shun::flow::Flow;
    use shun::payload::{PayloadEntry, PayloadSource};
    use shun::targets::install::{
        InstallContext, InstallFlow, VerbSpec, VerbTarget, WindowsRegistration, arp_key_path,
        uninstall,
    };

    /// Runs a full local install for `product` and uninstalls it when the
    /// test (or a panic) is done, so a red test cannot leave HKCU entries
    /// behind.
    struct Installed {
        ctx: InstallContext,
    }

    impl Installed {
        /// Installs into a fresh temp directory with per-test context
        /// tweaks (desktop shortcut, AUMID, verbs, …) applied.
        fn install(product: &str, tweak: &dyn Fn(&mut InstallContext)) -> (Self, PathBuf) {
            Self::in_dir(
                product,
                tempfile::tempdir().unwrap().path().join(stem(product)),
                tweak,
            )
        }

        /// Installs into a caller-chosen directory (non-ASCII paths and
        /// stem-sanitization cases build their own).
        fn in_dir(
            product: &str,
            install_dir: PathBuf,
            tweak: &dyn Fn(&mut InstallContext),
        ) -> (Self, PathBuf) {
            let mut context = ctx(product, install_dir.clone(), false);
            tweak(&mut context);

            let payload = demo_payload();
            let flow = InstallFlow {
                payload: &payload,
                registration: &WindowsRegistration,
                ctx: context.clone(),
            };
            flow.run(&mut |_| {}).unwrap();
            (Self { ctx: context }, install_dir)
        }
    }

    impl Drop for Installed {
        fn drop(&mut self) {
            let _ = uninstall(&self.ctx, &WindowsRegistration);
        }
    }

    fn start_menu_programs() -> PathBuf {
        let appdata = std::env::var("APPDATA").unwrap();
        Path::new(&appdata).join(r"Microsoft\Windows\Start Menu\Programs")
    }

    /// The Start-menu shortcut path for a product (stem-sanitized).
    fn start_menu_lnk(product: &str) -> PathBuf {
        start_menu_programs().join(format!("{}.lnk", stem(product)))
    }

    /// The desktop shortcut path for a product (stem-sanitized).
    fn desktop_lnk(product: &str) -> PathBuf {
        user_desktop().join(format!("{}.lnk", stem(product)))
    }

    /// Whether this machine allows `.lnk` creation on the desktop at
    /// all. Security policies (AV/EDR fake-shortcut protection) commonly
    /// deny exactly that; on such machines the desktop shortcut degrades
    /// to a warning and the assertions below follow the graceful path.
    fn desktop_accepts_lnk() -> bool {
        let probe = user_desktop().join("shun-write-probe.lnk");
        let ok = std::fs::write(&probe, b"probe").is_ok();
        let _ = std::fs::remove_file(&probe);
        ok
    }

    /// Tri-state AUMID probe. Security software may allow the `.lnk`
    /// file itself while denying the `IPropertyStore` write (0x80030005)
    /// — the exact split CI runners exhibit — so the stamp capability
    /// must be probed separately from the desktop policy. `None` means
    /// the probe could not even stage a `.lnk` (COM-less session, the
    /// stamp path never runs); assert nothing then.
    fn aumid_stamp_result() -> Option<bool> {
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path().join("aumid-probe.lnk");
        let exe = std::env::current_exe().unwrap();
        mslnk::ShellLink::new(&exe)
            .ok()?
            .create_lnk(&scratch)
            .ok()?;
        Some(shun::targets::aumid::stamp(&scratch, "celestia-island.ShunProbe").is_ok())
    }

    fn stem(product: &str) -> String {
        shun::targets::install::shortcut_stem(product)
    }

    /// Fields of a `.lnk` as the Windows shell resolves them, read back
    /// through PowerShell + the WScript.Shell COM parser (the same
    /// resolver Explorer uses when launching the shortcut).
    struct ShortcutView {
        target: String,
        working_dir: String,
        icon: String,
        arguments: String,
        aumid: String,
    }

    fn run_powershell(script: &str) -> String {
        // Windows PowerShell 5.1 decodes a BOM-less script in the ANSI
        // codepage — non-ASCII paths would mojibake. The BOM pins UTF-8.
        let script_dir = tempfile::tempdir().unwrap();
        let script_path = script_dir.path().join("read_shortcut.ps1");
        std::fs::write(
            &script_path,
            [b"\xEF\xBB\xBF".as_slice(), script.as_bytes()].concat(),
        )
        .unwrap();

        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&script_path)
            .output()
            .expect("spawn powershell");
        assert!(
            output.status.success(),
            "powershell failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    /// Whether this environment can resolve `.lnk` files through the
    /// WScript.Shell COM parser — probing from inside `dir_name`, so
    /// callers can also verify the session round-trips non-ASCII paths
    /// (CI service sessions may resolve ASCII but mojibake CJK).
    /// Interactive desktops can; headless sessions often cannot, so the
    /// COM round-trip tests skip there and stay green on real machines.
    fn shell_com_roundtrips(dir_name: &str) -> bool {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join(dir_name);
        if std::fs::create_dir_all(&nested).is_err() {
            return false;
        }
        let scratch = nested.join("probe.lnk");
        let exe = std::env::current_exe().unwrap();
        if mslnk::ShellLink::new(&exe)
            .unwrap()
            .create_lnk(&scratch)
            .is_err()
        {
            return false;
        }
        let script = format!(
            "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; \
             $s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); \
             Write-Output ($s.TargetPath -eq '{}')",
            scratch.display(),
            exe.display()
        );
        let out = run_powershell(&script);
        out.trim().ends_with("True")
    }

    fn shell_com_works() -> bool {
        shell_com_roundtrips("shun-com-probe")
    }

    fn read_shortcut_via_shell(lnk: &Path) -> ShortcutView {
        let script = format!(
            "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; \
             $s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); \
             $sh = New-Object -ComObject Shell.Application; \
             $it = $sh.Namespace((Split-Path '{}')).ParseName((Split-Path '{}' -Leaf)); \
             Write-Output (\"TARGET[\" + $s.TargetPath + \"]\"); \
             Write-Output (\"WORKDIR[\" + $s.WorkingDirectory + \"]\"); \
             Write-Output (\"ICON[\" + $s.IconLocation + \"]\"); \
             Write-Output (\"ARGS[\" + $s.Arguments + \"]\"); \
             Write-Output (\"AUMID[\" + $it.ExtendedProperty('System.AppUserModel.ID') + \"]\")",
            lnk.display(),
            lnk.display(),
            lnk.display()
        );
        let text = run_powershell(&script);
        let field = |name: &str| -> String {
            let line = text
                .lines()
                .find(|l| l.starts_with(name) && l.ends_with(']'))
                .unwrap_or_else(|| panic!("field {name} missing in powershell output: {text}"));
            line[name.len() + 1..line.len() - 1].to_string()
        };
        ShortcutView {
            target: field("TARGET"),
            working_dir: field("WORKDIR"),
            icon: field("ICON"),
            arguments: field("ARGS"),
            aumid: field("AUMID"),
        }
    }

    // ── Start-menu shortcut ────────────────────────────────────────────

    /// The shell must resolve the shortcut to the payload entry point
    /// with a usable working directory — the properties Explorer relies
    /// on when the user launches the app from the Start menu.
    #[test]
    fn start_menu_shortcut_resolves_through_the_shell() {
        if !shell_com_works() {
            eprintln!("skipping: no interactive shell COM in this session");
            return;
        }
        let product = "ShunDemo-Test-ShellLnk";
        let (_guard, install_dir) = Installed::install(product, &|_| {});

        let view = read_shortcut_via_shell(&start_menu_lnk(product));
        let expected_target = install_dir.join(r"bin\shun-demo.exe");
        assert_eq!(view.target, expected_target.display().to_string());
        assert_eq!(
            view.working_dir,
            install_dir.join("bin").display().to_string()
        );
        assert_eq!(view.arguments, "");
        // No explicit icon today: the shortcut falls back to the target
        // executable's own icon (mslnk writes no icon-location string).
        assert!(
            view.icon.is_empty() || view.icon == ",0",
            "unexpected icon location: {}",
            view.icon
        );
        // No AUMID unless the context resolved one (apply_config or an
        // explicit ctx.aumid).
        assert_eq!(view.aumid, "", "no AUMID without one configured");
    }

    /// Non-ASCII install paths (common on Chinese-locale machines) must
    /// round-trip through the synthetic PIDL that mslnk builds.
    #[test]
    fn shortcut_survives_a_non_ascii_install_path() {
        // Some CI sessions resolve ASCII paths but mojibake CJK ones —
        // probe with the very directory shape under test.
        if !shell_com_roundtrips("顺探测目录") {
            eprintln!("skipping: this session cannot round-trip non-ASCII paths through shell COM");
            return;
        }
        let dest = tempfile::tempdir().unwrap();
        let install_dir = dest.path().join("顺测试目录").join("ShunDemo-CJK");
        let product = "ShunDemo-Test-CJK";
        let (_guard, install_dir) = Installed::in_dir(product, install_dir, &|_| {});

        let view = read_shortcut_via_shell(&start_menu_lnk(product));
        assert_eq!(
            view.target,
            install_dir.join(r"bin\shun-demo.exe").display().to_string()
        );
    }

    /// Binary-level check against MS-SHLLINK: header size, the shell-link
    /// CLSID, and the exact flag set mslnk writes for a file target
    /// (target ID list + relative path + working dir + unicode — and,
    /// just as deliberately, NO link-info, name, arguments, or icon
    /// location block).
    #[test]
    fn lnk_bytes_match_the_ms_shllink_header() {
        let product = "ShunDemo-Test-Binary";
        let (_guard, _install_dir) = Installed::install(product, &|_| {});

        let bytes = std::fs::read(start_menu_lnk(product)).unwrap();
        assert_eq!(&bytes[0..4], &[0x4C, 0x00, 0x00, 0x00], "header size");
        #[rustfmt::skip]
        let clsid: [u8; 16] = [
            0x01, 0x14, 0x02, 0x00, // Data1 0x00021401, little-endian
            0x00, 0x00,             // Data2 0x0000
            0x00, 0x00,             // Data3 0x0000
            0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46, // Data4
        ];
        assert_eq!(&bytes[4..20], &clsid, "LinkCLSID");
        let flags = u32::from_le_bytes(bytes[0x14..0x18].try_into().unwrap());
        const HAS_TARGET_ID_LIST: u32 = 0x1;
        const HAS_RELATIVE_PATH: u32 = 0x8;
        const HAS_WORKING_DIR: u32 = 0x10;
        const IS_UNICODE: u32 = 0x80;
        assert_eq!(
            flags,
            HAS_TARGET_ID_LIST | HAS_RELATIVE_PATH | HAS_WORKING_DIR | IS_UNICODE,
            "LinkFlags"
        );
        let hotkey = u16::from_le_bytes(bytes[0x40..0x42].try_into().unwrap());
        assert_eq!(hotkey, 0, "no hotkey");
    }

    // ── ARP entry ──────────────────────────────────────────────────────

    /// The full NSIS-equivalent ARP field set, with the registry types
    /// Windows Settings consumes (DWORDs for the numeric flags).
    #[test]
    fn arp_entry_carries_the_full_nsis_field_set() {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

        let product = "ShunDemo-Test-Arp";
        let (_guard, install_dir) = Installed::install(product, &|_| {});

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let arp = hkcu
            .open_subkey_with_flags(arp_key_path(product), KEY_READ)
            .unwrap();

        let string = |name: &str| -> String { arp.get_value(name).unwrap() };
        assert_eq!(string("DisplayName"), product);
        assert_eq!(string("DisplayVersion"), "0.0.1");
        assert_eq!(string("Publisher"), "celestia-island");
        assert_eq!(
            string("InstallLocation"),
            install_dir.as_os_str().to_string_lossy(),
        );
        assert_eq!(
            string("DisplayIcon"),
            install_dir.join("uninstall.exe").display().to_string()
        );
        let uninstall_string = string("UninstallString");
        assert!(
            uninstall_string.starts_with('"') && uninstall_string.ends_with("\" /uninstall"),
            "UninstallString must quote the path (spaces): {uninstall_string}"
        );

        let dword = |name: &str| -> u32 { arp.get_value(name).unwrap() };
        assert_eq!(dword("NoModify"), 1);
        assert_eq!(dword("NoRepair"), 1);
        let payload = demo_payload();
        let expected_kb = (PayloadEntry::total_bytes(payload.manifest()) / 1024) as u32;
        assert_eq!(dword("EstimatedSize"), expected_kb);
    }

    // ── Desktop shortcut, AUMID, Explorer verbs ────────────────────────

    fn lnk_set(dir: &Path) -> BTreeSet<String> {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.to_lowercase().ends_with(".lnk"))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn user_desktop() -> PathBuf {
        // The test compares against the known-folder desktop; on this
        // class of machine it equals the USERPROFILE path (redirected
        // desktops are why the implementation never guesses like this).
        PathBuf::from(std::env::var("USERPROFILE").unwrap()).join("Desktop")
    }

    fn taskbar_pins() -> BTreeSet<String> {
        let pins = PathBuf::from(std::env::var("APPDATA").unwrap())
            .join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar");
        lnk_set(&pins)
    }

    /// The opt-in contract: with the extras unconfigured (the
    /// `InstallContext::new` defaults), registration writes the
    /// start-menu shortcut and the ARP entry — and nothing else. No
    /// desktop shortcut, no taskbar pin (there is no supported program-
    /// matic pinning anywhere), no Explorer verb, no protocol class.
    #[test]
    fn unconfigured_extras_stay_absent() {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

        let product = "ShunDemo-Test-Inventory";

        // A dedicated entry point so the Applications-key absence check
        // cannot race a parallel test registering verbs for the shared
        // `shun-demo.exe`.
        let (_guard, _install_dir) = Installed::install(product, &|ctx| {
            ctx.main_exe = Some("bin/absent-probe.exe".into())
        });

        // Shared-surface absences are checked by name, not by diffing
        // directory snapshots: parallel tests legitimately create their
        // own desktop shortcuts, which would race any before/after set
        // comparison.
        assert!(
            !desktop_lnk(product).exists(),
            "no desktop shortcut for this product"
        );
        assert!(
            !taskbar_pins().contains(&format!("{}.lnk", stem(product))),
            "no taskbar pin for this product"
        );

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        assert!(
            hkcu.open_subkey_with_flags(
                r"Software\Classes\Applications\absent-probe.exe",
                KEY_READ
            )
            .is_err(),
            "no Explorer verb under Classes\\Applications"
        );
        assert!(
            hkcu.open_subkey_with_flags(format!(r"Software\Classes\{product}"), KEY_READ)
                .is_err(),
            "no protocol / deep-link class registered"
        );
    }

    /// `desktop_shortcut: true` lands a second `.lnk` on the real
    /// desktop (the known folder, not a guessed path), resolves through
    /// the shell to the same entry point, and disappears on uninstall.
    /// On machines whose security policy denies desktop `.lnk` creation,
    /// the install must still succeed (start menu intact) — the desktop
    /// shortcut degrades to a warning.
    #[test]
    fn desktop_shortcut_lands_resolves_and_uninstalls() {
        let com_works = shell_com_works();
        let product = "ShunDemo-Test-Desktop";
        let writable = desktop_accepts_lnk() && com_works;
        let (guard, install_dir) = Installed::install(product, &|ctx| {
            ctx.desktop_shortcut = true;
        });

        let link = desktop_lnk(product);
        if writable {
            let view = read_shortcut_via_shell(&link);
            assert_eq!(
                view.target,
                install_dir.join(r"bin\shun-demo.exe").display().to_string()
            );
            assert_eq!(
                view.working_dir,
                install_dir.join("bin").display().to_string()
            );
            uninstall(&guard.ctx, &WindowsRegistration).unwrap();
            assert!(!link.exists(), "desktop shortcut removed by uninstall");
        } else {
            eprintln!(
                "note: this machine denies desktop .lnk creation; \
                 asserting the graceful-degradation path"
            );
            assert!(!link.exists());
            assert!(
                start_menu_lnk(product).exists(),
                "start-menu shortcut intact despite the desktop block"
            );
            uninstall(&guard.ctx, &WindowsRegistration).unwrap();
        }
        drop(guard); // the Drop uninstall is tolerant of the cleaned state
    }

    /// The AUMID is stamped on the shortcut through the Shell property
    /// store — the identity Windows uses for taskbar grouping and
    /// user-initiated pinning (there is no programmatic pinning).
    /// Whether this machine allows COM property-store writes on `.lnk`
    /// files — the same security policies that block desktop `.lnk`
    /// creation (fake-shortcut / ransomware protection) commonly deny
    /// `IPropertyStore::SetValue` on links, which downgrades the AUMID
    /// stamp to a warning.
    fn aumid_stamps_work() -> bool {
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path().join("probe.lnk");
        let exe = std::env::current_exe().unwrap();
        if mslnk::ShellLink::new(&exe)
            .unwrap()
            .create_lnk(&scratch)
            .is_err()
        {
            return false;
        }
        shun::targets::aumid::stamp(&scratch, "shun.probe").is_ok()
    }

    #[test]
    fn aumid_is_stamped_on_the_start_menu_shortcut() {
        if !shell_com_works() {
            eprintln!("skipping: no interactive shell COM in this session");
            return;
        }
        let product = "ShunDemo-Test-Aumid";
        let (_guard, _install_dir) = Installed::install(product, &|ctx| {
            ctx.aumid = Some("celestia-island.ShunDemoTest".into());
        });

        let view = read_shortcut_via_shell(&start_menu_lnk(product));
        if aumid_stamps_work() {
            assert_eq!(
                view.aumid, "celestia-island.ShunDemoTest",
                "AUMID readable through the shell property system"
            );
        } else {
            eprintln!(
                "note: this machine denies COM property writes on .lnk;                  asserting the graceful-degradation path"
            );
            assert!(
                start_menu_lnk(product).exists(),
                "the shortcut itself survives the failed stamp"
            );
            assert_eq!(view.aumid, "", "no stamp, but the link is intact");
        }
    }

    /// Context-menu verbs register under the per-user Application
    /// Registration key (no elevation), map all three verb targets to
    /// correct command lines, and uninstall removes them surgically.
    #[test]
    fn explorer_verbs_register_and_clean() {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

        let product = "ShunDemo-Test-Verbs";
        let (guard, install_dir) = Installed::install(product, &|ctx| {
            ctx.verbs = vec![
                VerbSpec {
                    key: "open-data".into(),
                    display: "打开数据目录".into(),
                    target: VerbTarget::DataFolder,
                },
                VerbSpec {
                    key: "safe-mode".into(),
                    display: "Safe mode".into(),
                    target: VerbTarget::App {
                        arguments: "--safe".into(),
                    },
                },
                VerbSpec {
                    key: "uninstall-it".into(),
                    display: "Uninstall".into(),
                    target: VerbTarget::Uninstall,
                },
            ];
        });

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let root = r"Software\Classes\Applications\shun-demo.exe\shell";
        let verb = |key: &str| -> (String, String) {
            let menu = hkcu
                .open_subkey_with_flags(format!(r"{root}\{key}"), KEY_READ)
                .unwrap();
            let command = hkcu
                .open_subkey_with_flags(format!(r"{root}\{key}\command"), KEY_READ)
                .unwrap();
            (menu.get_value("").unwrap(), command.get_value("").unwrap())
        };

        let (display, command) = verb("open-data");
        assert_eq!(display, "打开数据目录");
        assert_eq!(
            command,
            format!("explorer.exe \"{}\"", install_dir.display())
        );
        let (_display, command) = verb("safe-mode");
        // Registry verb commands keep the raw payload path spelling (no
        // shell normalization happens on the way in).
        assert_eq!(
            command,
            format!(
                "\"{}\" --safe",
                install_dir.join("bin/shun-demo.exe").display()
            )
        );
        let (_display, command) = verb("uninstall-it");
        assert_eq!(
            command,
            format!(
                "\"{}\" /uninstall",
                install_dir.join("uninstall.exe").display()
            )
        );

        uninstall(&guard.ctx, &WindowsRegistration).unwrap();
        assert!(
            hkcu.open_subkey_with_flags(root, KEY_READ).is_err(),
            "the Applications verb tree is gone after uninstall"
        );
        drop(guard); // tolerant re-uninstall
    }

    /// Machine scope registers the same surfaces machine-wide: the ARP
    /// entry lands under HKLM and the shortcut in the all-users Start
    /// Menu. Needs an elevated runner — skipped (with a note) otherwise;
    /// run elevated cargo to exercise it for real.
    #[test]
    fn machine_scope_writes_hklm_and_all_users_start_menu() {
        use shun::targets::elevate;
        use winreg::RegKey;
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

        // Opt-in: CI Windows runners are administrators by default, so
        // elevation alone must not unleash HKLM writes — set
        // SHUN_TEST_MACHINE_SCOPE=1 to run this for real.
        let opted_in = std::env::var_os("SHUN_TEST_MACHINE_SCOPE").is_some();
        if !opted_in || !elevate::is_elevated() || !shell_com_works() {
            eprintln!(
                "skipping: set SHUN_TEST_MACHINE_SCOPE=1 in an elevated,                  interactive shell to exercise machine-scope registration"
            );
            return;
        }
        let product = "ShunDemo-Test-Machine";
        let (guard, install_dir) = Installed::install(product, &|ctx| {
            ctx.scope = shun::targets::install::InstallScope::Machine;
        });

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let arp = hklm
            .open_subkey_with_flags(arp_key_path(product), KEY_READ)
            .unwrap();
        let name: String = arp.get_value("DisplayName").unwrap();
        assert_eq!(name, product);

        let programs = std::env::var("ProgramData")
            .map(|root| Path::new(&root).join(r"Microsoft\Windows\Start Menu\Programs"))
            .unwrap();
        let link = programs.join(format!("{}.lnk", stem(product)));
        assert!(link.exists(), "all-users start-menu shortcut");
        let view = read_shortcut_via_shell(&link);
        assert_eq!(
            view.target,
            install_dir.join("bin/shun-demo.exe").display().to_string()
        );

        uninstall(&guard.ctx, &WindowsRegistration).unwrap();
        assert!(
            hklm.open_subkey_with_flags(arp_key_path(product), KEY_READ)
                .is_err(),
            "HKLM ARP removed by uninstall"
        );
        assert!(!link.exists());
        drop(guard);
    }

    /// Degradations surface as warning records: on machines whose
    /// security policy denies desktop `.lnk` creation (or AUMID stamps),
    /// the install still succeeds and the wizard terminal explains why
    /// the shortcut is missing. On unrestricted machines the same
    /// install produces no warnings — both outcomes assert.
    #[test]
    fn degradations_stream_warning_records() {
        let product = "ShunDemo-Test-Warn";
        let mut events: Vec<shun::flow::FlowEvent> = Vec::new();
        {
            let mut context = ctx(
                product,
                tempfile::tempdir().unwrap().keep().join(product),
                false,
            );
            context.desktop_shortcut = true;
            context.aumid = Some("celestia-island.Warn".into());

            let payload = demo_payload();
            let flow = InstallFlow {
                payload: &payload,
                registration: &WindowsRegistration,
                ctx: context.clone(),
            };
            flow.run(&mut |event| events.push(event)).unwrap();
            uninstall(&context, &WindowsRegistration).unwrap();
        }

        let warnings: Vec<&str> = events
            .iter()
            .filter_map(|event| match event {
                shun::flow::FlowEvent::Log {
                    record: shun::flow::FlowLog::Warning { code, .. },
                } => Some(code.as_str()),
                _ => None,
            })
            .collect();
        if desktop_accepts_lnk() {
            assert!(
                !warnings.contains(&"desktop-shortcut-blocked"),
                "desktop writes are allowed here, yet warned: {warnings:?}"
            );
        } else {
            assert!(
                warnings.contains(&"desktop-shortcut-blocked"),
                "the desktop denial must explain itself: {warnings:?}"
            );
        }
        match aumid_stamp_result() {
            Some(true) => assert!(
                !warnings.contains(&"aumid-stamp-blocked"),
                "AUMID stamps land here, yet warned: {warnings:?}"
            ),
            Some(false) => assert!(
                warnings.contains(&"aumid-stamp-blocked"),
                "the AUMID denial must explain itself: {warnings:?}"
            ),
            None => {}
        }
        // Whatever the machine policy, the registration itself held.
        assert!(start_menu_lnk(product).exists() || !desktop_accepts_lnk());
    }

    /// Deep-link schemes register as per-user protocol handlers (the
    /// empty `URL Protocol` value marking a protocol class, the open
    /// command receiving the URL as `%1`), and uninstall removes them.
    #[test]
    fn deep_links_register_and_clean() {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

        let product = "ShunDemo-Test-Deep";
        let scheme = "shundemo-test";
        let (guard, install_dir) = Installed::install(product, &|ctx| {
            ctx.deep_links = vec![scheme.to_string()];
        });

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let class = hkcu
            .open_subkey_with_flags(format!(r"Software\Classes\{scheme}"), KEY_READ)
            .unwrap();
        let protocol: String = class.get_value("URL Protocol").unwrap();
        assert_eq!(protocol, "", "the marker value of a protocol class");
        let name: String = class.get_value("").unwrap();
        assert!(name.contains(product), "friendly name: {name}");
        let open = hkcu
            .open_subkey_with_flags(
                format!(r"Software\Classes\{scheme}\shell\open\command"),
                KEY_READ,
            )
            .unwrap();
        let command: String = open.get_value("").unwrap();
        assert_eq!(
            command,
            format!(
                "\"{}\" \"%1\"",
                install_dir.join("bin/shun-demo.exe").display()
            )
        );

        uninstall(&guard.ctx, &WindowsRegistration).unwrap();
        assert!(
            hkcu.open_subkey_with_flags(format!(r"Software\Classes\{scheme}"), KEY_READ)
                .is_err(),
            "protocol class removed by uninstall"
        );
        drop(guard);
    }

    /// Product names carrying filename-illegal characters must not break
    /// shortcut creation or nest ARP subkeys — both surfaces use the
    /// sanitized stem.
    #[test]
    fn illegal_product_names_are_stem_sanitized() {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
        if !shell_com_works() {
            eprintln!("skipping: no interactive shell COM in this session");
            return;
        }

        let product = "ShunDemo/Test *2";
        let (guard, install_dir) = Installed::install(product, &|_| {});

        let expected_stem = "ShunDemo_Test _2";
        let link = start_menu_programs().join(format!("{expected_stem}.lnk"));
        assert!(link.exists(), "sanitized start-menu shortcut");
        let view = read_shortcut_via_shell(&link);
        assert_eq!(
            view.target,
            install_dir.join(r"bin\shun-demo.exe").display().to_string()
        );

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        assert!(
            hkcu.open_subkey_with_flags(arp_key_path(product), KEY_READ)
                .is_ok(),
            "ARP key uses the sanitized stem"
        );

        uninstall(&guard.ctx, &WindowsRegistration).unwrap();
        assert!(!link.exists());
        drop(guard);
    }
}
