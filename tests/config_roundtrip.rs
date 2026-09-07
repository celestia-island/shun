use std::collections::BTreeMap;

use shun::config::{
    CustomStepConfig, FlashConfig, InstallConfig, ProductIdentity, ShunConfig, SourceConfig,
    TargetConfig, ThemeConfig, ThemeMode, Webview2Strategy,
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
        }),
        source: Some(SourceConfig::Online {
            url: "https://example.test/ShunDemo.shun".into(),
        }),
        license: Some("docs/LICENSE.md".into()),
        license_locales: BTreeMap::from([("zh-Hans".into(), "docs/LICENSE.zh.md".into())]),
        custom_steps: vec![CustomStepConfig {
            key: "whats-new".into(),
            after: "license".into(),
            title: "What's New".into(),
            markdown: "whats-new.md".into(),
        }],
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
