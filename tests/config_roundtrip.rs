use std::collections::BTreeMap;

use shun::config::{
    CustomStepConfig, FlashConfig, InstallConfig, ProductIdentity, ShunConfig, SigningConfig,
    SourceConfig, TargetConfig, ThemeConfig, ThemeMode, Webview2Strategy,
};

fn sample() -> ShunConfig {
    ShunConfig {
        product: ProductIdentity {
            name: "ShunDemo".into(),
            version: "0.1.0".into(),
            publisher: Some("celestia-island".into()),
            logo: Some("logo.webp".into()),
        },
        payload: Some("examples/demo_payload".into()),
        webview2: Some(Webview2Strategy::FixedVersion {
            path: "WebView2Runtime".into(),
        }),
        targets: vec![
            TargetConfig::Install(InstallConfig::default()),
            TargetConfig::Flash(FlashConfig::default()),
        ],
        shell: Some(shun::config::ShellUiConfig {
            timeline: Some(shun::config::TimelineOrientation::Left),
            theme: Some(ThemeConfig {
                mode: Some(ThemeMode::Dark),
                accent: Some([34, 211, 238]),
            }),
            language: Some("zh-Hans".into()),
            log_level: Some(shun::config::LogVerbosity::Scripts),
        }),
        source: Some(SourceConfig::Online {
            url: "https://example.test/ShunDemo.shun".into(),
        }),
        attachments: vec![shun::config::AttachmentConfig {
            key: "models".into(),
            title: "2D/3D model pack".into(),
            dest: "models".into(),
            size: Some(123),
            online: shun::config::AttachmentOnlineConfig {
                url: "https://example.test/models.shun".into(),
            },
        }],
        license: Some("docs/LICENSE.md".into()),
        license_locales: BTreeMap::from([("zh-Hans".into(), "docs/LICENSE.zh.md".into())]),
        custom_steps: vec![CustomStepConfig {
            key: "whats-new".into(),
            after: "license".into(),
            title: "What's New".into(),
            markdown: "whats-new.md".into(),
        }],
        steps: None,
        msix: Some(shun::config::MsixConfig {
            identity_name: "ShunDemo".into(),
            publisher: "CN=celestia-island".into(),
            display_name: "ShunDemo".into(),
            description: Some("Shun delivery demo".into()),
            executable: Some("bin/shun-demo.exe".into()),
            logo_background: Some("#0F172A".into()),
        }),
        signing: Some(SigningConfig {
            windows: Some(shun::config::WindowsSigningConfig {
                enabled: true,
                thumbprint: Some("0123456789abcdef".into()),
                timestamp_url: "http://timestamp.digicert.com".into(),
            }),
            macos: None,
        }),
    }
}

#[test]
fn config_roundtrips_through_json() {
    let cfg = sample();
    let json = serde_json::to_string(&cfg).unwrap();
    let back: ShunConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg, back);
}

#[test]
fn webview2_strategy_uses_kebab_case_tags() {
    let json = serde_json::to_value(sample()).unwrap();
    assert_eq!(json["webview2"]["type"], "fixed-version");
}

#[test]
fn fixed_version_carries_path() {
    let json = serde_json::to_value(sample()).unwrap();
    assert_eq!(json["webview2"]["path"], "WebView2Runtime");
}

#[test]
fn targets_use_kebab_case_kinds() {
    let json = serde_json::to_value(sample()).unwrap();
    assert_eq!(json["targets"][0]["kind"], "install");
    assert_eq!(json["targets"][1]["kind"], "flash");
}

#[test]
fn install_modes_default_to_enabled() {
    let json = serde_json::to_value(sample()).unwrap();
    assert_eq!(json["targets"][0]["local"], true);
    assert_eq!(json["targets"][0]["portable"], true);
}

#[test]
fn cargo_manifest_top_level_main_exe_feeds_the_install_target() {
    // `[package.metadata.shun] main-exe` + an `[install]` table: the
    // entry point must survive the merge instead of being dropped.
    let manifest = std::env::current_dir()
        .unwrap()
        .join("demo-app")
        .join("Cargo.toml");
    let config = shun::config::ShunConfig::from_cargo_manifest(&manifest).unwrap();
    let install = config
        .targets
        .iter()
        .find_map(|t| match t {
            shun::config::TargetConfig::Install(install) => Some(install),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        install.main_exe.as_deref(),
        Some(std::path::Path::new("bin/shun-demo.exe"))
    );
}
