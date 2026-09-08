//! MSIX packaging — the generated `AppxManifest.xml` and, when the
//! Windows SDK is present, a real MakeAppx pack round-trip.

use std::path::Path;

use shun::msix::{MsixInputs, build_msix, find_makeappx, write_manifest};

fn inputs<'a>() -> MsixInputs<'a> {
    MsixInputs {
        identity_name: "ShunDemo-Test",
        publisher: "CN=celestia-island",
        display_name: "ShunDemo & <friends>",
        description: r#"A "demo" <payload> & friends"#,
        version: "0.1.0",
        executable: Path::new(r"bin\shun-demo.exe"),
        logo_png: Some(&[0x89, b'P', b'N', b'G']),
        logo_background: Some("#0F172A"),
    }
}

/// The manifest is the whole packaging contract: identity, padded
/// four-part version, full-trust entry point with forward-slash paths,
/// and XML-escaped display strings.
#[test]
fn appxmanifest_declares_identity_entry_and_escaping() {
    let staging = tempfile::tempdir().unwrap();
    write_manifest(staging.path(), &inputs()).unwrap();

    let manifest = std::fs::read_to_string(staging.path().join("AppxManifest.xml")).unwrap();
    assert!(manifest.contains(r#"Identity Name="ShunDemo-Test""#));
    assert!(manifest.contains(r#"Publisher="CN=celestia-island""#));
    assert!(
        manifest.contains(r#"Version="0.1.0.0""#),
        "version padded to four parts"
    );
    assert!(
        manifest.contains(r#"Executable="bin/shun-demo.exe""#),
        "backslashes normalized"
    );
    assert!(manifest.contains(r#"DisplayName="ShunDemo &amp; &lt;friends&gt;""#));
    assert!(manifest.contains(r#"Description="A &quot;demo&quot; &lt;payload&gt; &amp; friends""#));
    assert!(manifest.contains(r##"BackgroundColor="#0F172A""##));
    assert!(manifest.contains(r#"Name="runFullTrust""#));
    assert!(manifest.contains(r#"MinVersion="10.0.17763.0""#));

    // The logo bytes land beside the manifest, flattened as declared.
    let logo = std::fs::read(staging.path().join("assets").join("logo.png")).unwrap();
    assert_eq!(logo, [0x89, b'P', b'N', b'G']);
}

/// End-to-end through the Windows SDK packer when installed: a packed
/// `.msix` is an OPC zip carrying the manifest and content types. Skips
/// (with a note) on machines without the SDK.
#[test]
fn makeappx_packs_a_real_package() {
    let Some(_makeappx) = find_makeappx() else {
        eprintln!("skipping: MakeAppx.exe not found (no Windows SDK on this machine)");
        return;
    };

    let payload_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(payload_dir.path().join("bin")).unwrap();
    std::fs::write(
        payload_dir.path().join("bin").join("shun-demo.exe"),
        b"MZ fake exe",
    )
    .unwrap();

    let out_dir = tempfile::tempdir().unwrap();
    let out = out_dir.path().join("ShunDemo-Test.msix");
    build_msix(payload_dir.path(), &out, &inputs()).unwrap();

    assert!(out.exists(), "package written");
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[0..2], b"PK", "OPC/zip magic");
    assert!(
        bytes
            .windows(b"AppxManifest.xml".len())
            .any(|w| w == b"AppxManifest.xml")
    );
    assert!(
        bytes
            .windows(b"[Content_Types].xml".len())
            .any(|w| w == b"[Content_Types].xml"),
        "OPC content types stream"
    );
}
