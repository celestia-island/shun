# Design note: cross-platform delivery

Status: **Windows surface verified and its gaps closed (2026-09);
Linux and macOS runtime registration backends implemented; mobile
(Android/iOS) and HarmonyOS researched — out of scope for now.** This
note records (a) what a concentrated test of the auto-packaging
registration surface proved about the shipped Windows backend, (b) the
formerly-missing desktop-shortcut / taskbar-identity / context-menu
surfaces — now implemented, with the real-machine security-policy
findings that shaped them, (c) the Linux and macOS backends, and (d)
the feasibility verdict for Android, iOS, and HarmonyOS (deferred).

The executable inventory of (a) lives in
`tests/registration_shortcuts.rs` and `tests/msix_pack.rs`.

## 1. What the concentrated Windows test proved (2026-09, real machine)

Verified by running the real `InstallFlow` on a Windows 11 machine and
reading the results back through Windows itself (WScript.Shell COM
resolver, registry, MakeAppx from the Windows SDK) — not by inspecting
our own output:

| Surface | Result |
| --- | --- |
| Start-menu `.lnk` resolves through the shell | **ok** — `TargetPath` and `WorkingDirectory` come back exactly as the install context declared; `mslnk`'s hand-built synthetic PIDL resolves correctly |
| Non-ASCII install paths (中文目录) | **ok** — a `顺测试目录` install dir round-trips through the COM resolver byte-exact |
| `.lnk` binary vs MS-SHLLINK | **ok** — header, CLSID `{00021401-…}`, flag set (target ID list + relative path + working dir + unicode), no hotkey |
| ARP entry (HKCU) | **ok** — full NSIS-equivalent field set: DisplayName/Version/Publisher/InstallLocation/DisplayIcon/UninstallString (quoted), `NoModify`/`NoRepair`/`EstimatedSize` as DWORDs; EstimatedSize matches the payload manifest total |
| Uninstall cleanup | **ok** — ARP key, shortcut, payload, uninstaller, directory all removed (covered by `tests/install_local.rs`, re-confirmed here) |
| MSIX manifest generation | **ok** — identity, four-part padded version, XML-escaped strings, forward-slash entry point, runFullTrust |
| MSIX real pack (MakeAppx 10.0.26100) | **ok** — valid OPC zip with `[Content_Types].xml` + `AppxManifest.xml`; the produced `dist/shundemo-0.1.0-x64.msix` carries **no signature block** (by design: Store distribution signs it, or a self-signed cert must be trusted) |

## 2. The Windows gap-closing pass (implemented 2026-09)

Everything below landed after the verification pass, each surface
exercised by the registration test suite on a real machine.

### Desktop shortcut — `install.desktop-shortcut`

Resolved from the policy (`always` | `never` | `ask`, the NSIS checkbox
convention — the egui wizard shows a default-checked toggle for `ask`,
headless runs answer checked) and written beside the start-menu one.
The desktop is resolved via **`SHGetKnownFolderPath(FOLDERID_Desktop)`**
— never `%USERPROFILE%\Desktop`, which is wrong whenever the desktop is
redirected (OneDrive, domain policies). Uninstall removes it
unconditionally (the config may have changed between install and
uninstall).

**Measured on a real machine**: security policies (AV/EDR
fake-shortcut and ransomware protection) commonly deny `.lnk` creation
on the desktop *specifically* — on the verification machine even
`echo x > Desktop\probe.lnk` from an elevated shell is denied while
`.tmp` files write freely. Therefore the desktop shortcut is
**best-effort**: a denied write downgrades to a warning and never fails
the install (the start-menu shortcut and ARP entry are the critical
surface). The test suite probes the machine's policy and asserts both
the happy path and the graceful-degradation path.

### Taskbar identity — `install.aumid`

Programmatic taskbar pinning stays **blocked by platform design** (no
supported API; the pin hacks were removed in Windows 10). What shipped
is the identity half: every shortcut is stamped with a
**`System.AppUserModel.ID`** through the Shell COM property store
(`IShellLink` → `IPersistFile` → `IPropertyStore`, in
`src/targets/aumid.rs` — `mslnk` writes bytes only), so taskbar
grouping, jump lists, and *user-initiated* pinning behave. The default
AUMID is generated as `{publisher}.{product}`; `install.aumid`
overrides it, and the app should pass the same value to
`SetCurrentProcessExplicitAppUserModelID`. MSIX installs get identity
for free through the package. Stamping is best-effort for the same
policy reason (the verification machine denies
`IPropertyStore::SetValue` on `.lnk` files — 0x80030005 — so the stamp
downgrades to a warning there).

### Context-menu verbs — `[[install.verbs]]`

Tier 1 shipped: per-user Explorer verbs under
`HKCU\Software\Classes\Applications\<exe>\shell\<verb>\command` — the
documented Application Registration surface, no elevation, surfaced on
the app's exe and shortcuts to it. Three verb targets map to command
lines on every platform that implements them:
`data-folder` (opens the install directory), `uninstall` (runs the
copied uninstaller), `app` (entry point + arguments). Uninstall removes
the verb keys it created, then the `shell`/`Applications` containers
only where empty (a verb someone else registered survives). Tier 2
(file-type associations) and tier 3 (MSIX `FileExplorerExtension`)
remain future work.

### Deep links — `install.deep-links`

The delivery model promised them since the first draft; they are real
now, on every backend, per-user: Windows registers each scheme as a
protocol class under `HKCU\Software\Classes\<scheme>` (the empty `URL
Protocol` marker + an open command that receives the URL as `%1`);
Linux declares `MimeType=x-scheme-handler/<scheme>;` on the launcher
and claims the default via `xdg-mime`; a synthesized macOS
`Info.plist` carries `CFBundleURLTypes`. Schemes normalize to
lowercase `[a-z0-9+.-]` (`"MyApp://"` → `myapp`). Uninstall deletes
the Windows protocol class; removing the Linux launcher orphans the
handler (the `mimeapps.list` line becomes inert — noted, accepted).

### Why the installer does not request UAC elevation

The question came up after the desktop-shortcut findings, and the
answer has three legs:

1. **Everything shun writes is per-user surface** — HKCU, the user's
   Start Menu and desktop, `%LOCALAPPDATA%`. None of it needs an
   elevated token, so a UAC prompt would buy nothing while adding the
   worst kind of prompt noise (training users to click through). This
   is the same trade the wowsp NSIS template makes, and the same one
   behind VS Code / Chrome *user* setups.
2. **Elevation would not fix the `.lnk` blocking anyway**: the block
   is a security product's file-system filter keyed on the desktop
   folder and the `.lnk` extension — filters intercept processes by
   policy, not by ACL, and elevated processes are filtered too. (The
   Windows-controlled-folder case behaves the same: it blocks admins
   unless the app is allow-listed.) The correct response is the one
   shipped: degrade to a warning, keep the start-menu shortcut and
   ARP intact.
3. **Elevated writes to *user* surfaces are a correctness trap**: an
   elevated process resolves profiles differently (an admin account's
   desktop, `%APPDATA%`, and registry hive can all differ from the
   installing user's) — the classic NSIS all-users-shortcuts bug.
   When elevation is genuinely needed, the specific step elevates
   *itself*: the WebView2 Evergreen bootstrapper carries its own
   `requireAdministrator` manifest, so the shell stays `asInvoker` and
   delegates.

A machine-wide scope (`Program Files`, HKLM ARP, all-users shortcuts)
is a legitimate *mode* some products need — it shipped as exactly
that: a deliberate opt-in (`install.scope`), never the default. See
section 6.

### Robustness findings, both fixed

- Product names with filename-illegal characters (`/\:*?"<>|`, trailing
  dots/spaces) are **stem-sanitized** for every filesystem and registry
  surface (`.lnk` names, ARP key path) — a `\` in a product name no
  longer nests registry subkeys.
- The ARP `UninstallString` passes `/uninstall`, but the installer
  shell's headless parser only accepted `--uninstall` — clicking
  "Uninstall" in Windows Settings launched the wizard instead of
  uninstalling. Both spellings now parse.

## 3. Linux and macOS (runtime backends implemented; packaging
## outputs next)

**Both are tractable, and both share one hard limit with Windows:
programmatic taskbar/dock pinning does not exist anywhere.** The
runtime `Registration` backends shipped; the build-side packaging
outputs (deb/rpm via `tauri-bundler`, DMG) remain follow-up work
because they need their native build hosts.

### Linux — `LinuxRegistration` (src/targets/freedesktop.rs)

Everything the Windows backend does maps onto freedesktop conventions,
all per-user (`~/.local/share/...`), no elevation:

- **launcher registration** = write `<product>.desktop` (Name, Exec,
  Icon from `install.icon`, Categories, and — critically —
  **`StartupWMClass`** = the entry executable stem, the field that
  makes a user-initiated taskbar/dock pin group under the right icon)
  into `~/.local/share/applications`, then run
  `update-desktop-database` on it (safe to skip when the tool is
  absent — DEs re-scan lazily);
- **context-menu verbs + uninstall entry** = `Actions=` +
  `[Desktop Action <id>]` groups — always including an **Uninstall**
  action, because GNOME Software / KDE Discover only list apps their
  own package backends track: a shun-installed app never appears there.
  The three verb targets map to `xdg-open`, the uninstaller, and the
  entry point + arguments;
- **exec-bit restore** — the payload archive carries 0644 for every
  entry, so the backend chmods the entry point and the copied
  uninstaller back to 0755;
- **unregister** = delete the `.desktop` + refresh the database.

The `.desktop` writer is plain data plumbing that compiles (and is
unit-tested) on every platform; only the process-spawning half is
Linux-gated. Taskbar/dock **pinning stays impossible** (no cross-
desktop API: GNOME favorites are an internal gsettings key, KDE pins
live in undocumented appletsrc; treat as user action). File-manager
context menus (Nautilus scripts / Dolphin service menus) stay out of
scope.

**Packaging formats** (still build-side, follow-up): **tarball/portable
(shun already has it) + deb ([cargo-deb]) + rpm
([cargo-generate-rpm])** is the best subset — exactly what
`tauri-bundler` emits (it is a library usable for non-Tauri payloads,
as is [cargo-packager]). AppImage = medium; Flatpak = medium-high;
snap = high, defer. The honest "runs on every distro" limit: glibc is
forward-compatible only and **musl statics cannot carry WebView apps**
(webkit2gtk drags the GTK C stack) — the delivery shell can be
musl-static, but delivered Tauri apps must build against the oldest
webkit2gtk-4.1 baseline supported (Ubuntu 22.04 / Debian 12 / Fedora
37+ era).

### macOS — `MacOSRegistration` (src/targets/macos.rs)

- **registration** = locate the `.app` bundle the entry executable
  lives in (nearest `.app` ancestor); synthesize a minimal
  `Info.plist` (`plist.rs`, pure and unit-tested everywhere) when the
  payload carried none; restore the entry point's exec bit; **strip
  inherited `com.apple.quarantine`** recursively from the install
  (browsers stamp the installer, and macOS copies preserve xattrs —
  without this the delivered app inherits Gatekeeper gating the user
  already answered); then `lsregister -f` the bundle — Spotlight and
  Launchpad follow. Unregister = `lsregister -u` (files are removed by
  the generic uninstall pass). Bare-executable payloads (no `.app`)
  register as a no-op — portable conventions;
- **Dock pinning**: **no supported API** (the `defaults write
  com.apple.dock` + `killall Dock` hack stomps user prefs and is
  unreliable on recent macOS) — Launchpad/Spotlight presence via LS
  registration is the discoverability equivalent;
- **signing stays mandatory process work** for real distribution:
  Developer ID + hardened runtime + `notarytool` + staple; and the
  downloaded shell **will be translocated** — handle self-path
  assumptions accordingly;
- **packaging outputs** (follow-up): DMG via tauri-bundler /
  `hdiutil`, `.pkg` only for admin flows, Homebrew cask as a channel.
  Ship **universal2** (dual build + `lipo` + re-sign).

[cargo-deb]: https://github.com/kornelski/cargo-deb
[cargo-generate-rpm]: https://crates.io/crates/cargo-generate-rpm
[cargo-packager]: https://github.com/crabnebula-dev/cargo-packager

## 4. Android and iOS (researched)

**Reframe first: on mobile, "installation" is platform-owned and
signature-verified. There is no equivalent of streaming a zstd tar of
an app directory to a user-chosen location.** shun's honest mobile role
is a **build/packaging/signing pipeline** (a "cargo-dist for mobile"),
not an installer runtime.

### Android

- **Packaging mechanics**: APK = signed zip (DEX + resources + per-ABI
  `.so`); pipeline `aapt2` → `d8`/`r8` → package → `zipalign` →
  `apksigner`. AAB is the Play publishing format (required for new
  apps); sideloading needs a concrete APK (`bundletool build-apks` +
  re-sign). Rust targets are Tier 2 with host tools.
- **Tooling that is alive (2025–2026)**: Tauri 2's `tauri android
  build` (wraps cargo-mobile2 + Gradle; keystore via
  `keystore.properties`), **cargo-ndk** (maintained), the **`apk`
  crate** (Gradle-free aapt2/d8/zipalign/apksigner). **xbuild and
  cargo-apk are dead/dormant** — do not build on them. Tauri 2 mobile
  is officially stable; Android side more mature than iOS.
- **Runtime installer: infeasible/pointless.** The OS package manager
  installs from a signed APK with a user-facing confirmation (silent
  only for device-owner/MDM); the APK *is* the installer. Play policy
  additionally **bans self-updating / downloading executable code** —
  a shun-style updater runtime would be a policy violation for any
  Play-distributed app. Sideloading degrades by design:
  **verified-developer sideloading enforcement starts 2026-09-30**
  (unverified apps hit a multi-step flow with a 24-hour wait).
- **Shortcuts**: home-screen icons and app shortcuts (`shortcuts.xml`,
  `ShortcutManager`, `requestPinShortcut`) are **app-declared only** —
  no installer-time injection API exists. The build-time analog is
  generating `shortcuts.xml`/intent-filters into the APK shun packs.

### iOS

- **Packaging mechanics**: IPA = zip with `Payload/App.app` +
  `embedded.mobileprovision` (App ID + entitlements + cert + Ad Hoc
  UDID allowlist). Channels: App Store, TestFlight, Ad Hoc (100
  devices/type/year), Enterprise. **The signing toolchain (codesign,
  xcodebuild, keychain) is macOS-only** — hard host requirement.
- **Runtime installer: infeasible** outside two niches: (a) Ad Hoc
  OTA manifests (`itms-services://?...manifest.plist`) — easy, legal,
  niche; (b) the EU Web Distribution / alternative-marketplace regime
  — real but gated behind Apple eligibility + notarization + the
  Oct 2026 fee terms (5% Core Technology Commission), and EU-only.
  Free-Apple-ID sideloading (AltStore/Sideloadly) is a 7-day/3-app
  hobbyist path, not a productizable channel.
- **Shortcuts**: nothing exists at installer time, in any channel —
  home-screen icons, URL schemes, universal links are app-declared
  and signature-validated.
- **egui fallback**: Android = `android-activity` + winit + wgpu
  (Vulkan/GLES); iOS = winit + wgpu (Metal) embedded into a UIKit
  host via FFI + Xcode project. Neither has a turnkey story —
  a shun packaging pipeline is exactly the missing piece.

## 5. HarmonyOS (researched)

**Verdict up front: "shun supports HarmonyOS" can honestly mean exactly
one thing in 2026 — a build-time packaging/signing target producing
release-signed HAP/APP artifacts, plus a `hdc install` dev-device flow.
A runtime installer (the NSIS half of shun) has no legal or technical
substrate on HarmonyOS NEXT.**

Landscape (2025–2026): HarmonyOS NEXT (5.0, Oct 2024) dropped the APK
compatibility layer; the line to target is HarmonyOS 6+ (API 20/23),
China-only, AppGallery-only, ~19% of China's OS market. OpenHarmony is
the open base; commercial HarmonyOS is Huawei's product on top — a
packager targets the commercial one.

- **Package format**: HAP (zip: `module.json5`, ArkTS bytecode, native
  `libs/<abi>/*.so`); HSP/HAR shared packages; `.app` = AppGallery
  submission pack (`pack.info`). Tooling is CLI-usable: `ohpm` +
  `hvigorw assembleHap` + `hap-sign-tool` + `app_packing_tool.jar`
  (officially supported headless CI builds).
- **Signing**: SHA256withECDSA; `.p12` keystore + `.cer` +
  `.p7b` profile (bundle name, permissions, and for debug the device
  UDID allowlist); certs are **issued by Huawei via AppGallery
  Connect** (individual registration free — no Apple-style fee).
- **Rust**: `aarch64/armv7/x86_64-unknown-linux-ohos` are **Tier 2
  with host tools** (rustup-ready since 1.78). Community `ohos.rs`
  toolchain (`cargo-ohos`, `napi-ohos`, `ohos-openssl`) is the glue;
  Rust-core + ArkTS-shell is a proven architecture (RustDesk OHOS).
  **egui is blocked**: winit has no upstream OHOS backend (community
  betas only). **Tauri**: an official-but-unmerged `feat/open-harmony`
  branch (wry/tao patches, `cargo tauri ohos` CLI) works today but
  moves fast — expect re-pinning every few months.
- **Distribution honesty**: consumer sideloading is effectively closed
  (AppGallery only; `hdc install` needs developer mode + Huawei
  signature + UDID). Designated-device release: 100 devices/year,
  90-day validity. Enterprise distribution currently scoped to Qingyun
  enterprise PCs. **HarmonyOS PC** is real (ARM, store-distributed,
  no sideloading yet) — Huawei has *stated intent* to open PC
  sideloading later; that is the one watch-item that could someday
  justify a desktop delivery runtime there.

Effort table: HAP packaging target **medium**; Rust cross step
**easy–medium** (SDK clang wrapper as linker, ohos-openssl for TLS);
Tauri-on-OHOS packaging **medium–hard** (unmerged upstream); egui
fallback **hard** (no winit); runtime installer/flasher
**infeasible**.

## 6. Install scope and the declarative wizard (implemented 2026-09)

### Install scope — `install.scope = user | machine | ask`

Per-user stays the default (see "Why the installer does not request UAC
elevation"). `machine` — or an `ask` answered with "all users" — flips
every registration surface to its machine-wide equivalent: the ARP
entry under **HKLM**, the shortcut in the **all-users Start Menu**
(`%ProgramData%`), the desktop shortcut on the **public desktop**
(`FOLDERID_PublicDesktop`), verbs and deep links under
`HKLM\Software\Classes`. The shell detects the resolution **before**
the flow runs and, when not yet elevated, re-launches itself via the
`runas` verb carrying the user's answers (`--silent --mode=… --dir=…
--scope=machine`): the UAC consent is the one prompt, shown only for
the mode that needs it — the bootstrapper pattern, exactly as
promised. Uninstall mirrors (the ARP `UninstallString` launches the
uninstaller, which elevates the same way). Machine scope is
Windows-only; the Linux/macOS backends reject it explicitly. The
integration test skips unless the runner is elevated (`cargo` from an
administrator shell exercises it).

### The wizard pipeline — `[[package.metadata.shun.steps]]`

The wizard is now a declarative, ordered, freely composed pipeline
instead of a fixed mode → install sequence. Five step kinds:

```toml
[[package.metadata.shun.steps]]
kind = "mode"                    # delivery mode + directory; embeds the
                                 # `ask` toggles (desktop shortcut, scope)
[[package.metadata.shun.steps]]
kind = "scope"                   # standalone user/machine choice
[[package.metadata.shun.steps]]
kind = "license"                 # license pane (license / license-locales)
[[package.metadata.shun.steps]]
kind = "content"                 # custom markdown pane
title = "Release notes"
markdown = "notes.md"         # relative to the manifest
[[package.metadata.shun.steps]]
kind = "install"                 # the delivery run (exactly one required)
```

Absent `steps` = the default pipeline (mode → license-when-declared →
install) with the legacy `custom-steps` injected after their `after`
keys; declaring both is a configuration error, as is zero or multiple
`install` steps. The scope question and the desktop-shortcut toggle
surface wherever the `ask` policies meet a pane: embedded in the mode
step, or standalone (`scope`) — the developer composes. Content and
license documents are read **at build time** and inlined into
`shun-steps.json` (`ShunConfig::resolve_steps`), so runtime installers
carry no file dependencies. The egui fallback renders the full
pipeline (per-step rail, license gating, back/next navigation); the
Tauri `ShellView` exposes the resolved steps for the web front-end.

## 7. Portfolio status (2026-09)

| Capability | Win | Linux | macOS | Android | iOS | HarmonyOS |
| --- | --- | --- | --- | --- | --- | --- |
| Runtime install + register | ✅ user + machine scope | ✅ `.desktop` backend (user) | ✅ `.app` backend (user) | deferred | deferred | deferred |
| Desktop/start-menu shortcut | ✅ both (policy-driven) | ✅ launcher entry | ✅ (Launchpad/Spotlight) | n/a | n/a | n/a |
| Taskbar/dock pinning | identity only (AUMID stamp) | identity only (StartupWMClass) | identity only (LS) | n/a | n/a | n/a |
| Right-click menu | ✅ tier-1 verbs | ✅ Desktop Actions | NSServices later | deferred | deferred | deferred |
| Deep links | ✅ protocol class | ✅ MimeType + xdg-mime | ✅ CFBundleURLTypes | deferred | deferred | deferred |
| Uninstall story | ✅ ARP + self-delete | ✅ Action-based | ✅ LS-unregister + files | OS | OS | OS |
| Packaging output | ✅ installer + MSIX | tarball ✅; deb/rpm next | bundle ✅; DMG next | deferred | deferred | deferred |
| Signing reality | Authenticode / Store | optional | mandatory (process work) | keystore / Play | Apple certs | Huawei AGC |

**Deferred by decision (2026-09)**: Android, iOS, and HarmonyOS — the
research in sections 4–5 stands; none of it is scheduled.

**Next in line**:

1. deb/rpm packaging outputs via `tauri-bundler` (Linux CI), DMG via
   the macOS host lane.
2. Windows context-menu tier 2 (file-type associations) if a consumer
   needs it.
3. **Never**: runtime installers / shortcut injection on any phone OS;
   taskbar/dock pinning anywhere; snap until it earns its complexity.
