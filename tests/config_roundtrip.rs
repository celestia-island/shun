use shun::config::{
    FlashConfig, InstallConfig, ProductIdentity, ShunConfig, TargetConfig, Webview2Strategy,
};

fn sample() -> ShunConfig {
    ShunConfig {
        product: ProductIdentity {
            name: "WoWSP".into(),
            logo: Some("logo.webp".into()),
        },
        webview2: Some(Webview2Strategy::FixedVersion {
            path: "WebView2Runtime".into(),
        }),
        targets: vec![
            TargetConfig::Install(InstallConfig::default()),
            TargetConfig::Flash(FlashConfig::default()),
        ],
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
