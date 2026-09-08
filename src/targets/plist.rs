//! Minimal `Info.plist` synthesis for macOS app bundles — plain XML
//! (the bundle's launch identity; Launch Services, Finder, and the Dock
//! all key off it). Rendering is pure data plumbing, so it compiles and
//! is unit-tested on every platform; only the bundle wiring in
//! [`crate::targets::macos`] is macOS-gated.

/// The default bundle identifier: `shun.{publisher}.{product}` with each
/// segment reduced to `[A-Za-z0-9-]` (bundle identifiers are dot-joined
/// identifiers, not free text).
pub fn bundle_identifier(publisher: Option<&str>, product: &str) -> String {
    let segment = |raw: &str| {
        let mapped: String = raw
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        joined(&mapped)
    };
    let product = segment(product);
    match publisher.map(segment).filter(|p| !p.is_empty()) {
        Some(publisher) => format!("shun.{publisher}.{product}"),
        None => format!("shun.{product}"),
    }
}

/// `-`-joined non-empty segments of a mapped identifier.
fn joined(mapped: &str) -> String {
    mapped
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>()
        .join("-")
}

/// Renders a minimal launch-capable `Info.plist` for a bundle whose
/// executable is `executable_name`. Deep-link schemes (when present)
/// are declared through `CFBundleURLTypes` — Launch Services' URL
/// registration.
pub fn render_info_plist(
    product: &str,
    version: &str,
    publisher: Option<&str>,
    executable_name: &str,
    deep_link_schemes: &[String],
) -> String {
    let entry = |key: &str, value: &str| {
        format!(
            "    <key>{key}</key>\n    <string>{}</string>\n",
            xml_escape(value)
        )
    };
    let mut doc = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n",
    );
    doc.push_str("    <key>CFBundlePackageType</key>\n    <string>APPL</string>\n");
    doc.push_str(&entry("CFBundleName", product));
    doc.push_str(&entry("CFBundleDisplayName", product));
    doc.push_str(&entry("CFBundleExecutable", executable_name));
    doc.push_str(&entry(
        "CFBundleIdentifier",
        &bundle_identifier(publisher, product),
    ));
    doc.push_str(&entry("CFBundleShortVersionString", version));
    doc.push_str(&entry("CFBundleVersion", version));
    if !deep_link_schemes.is_empty() {
        doc.push_str("    <key>CFBundleURLTypes</key>\n    <array>\n        <dict>\n");
        doc.push_str(&format!(
            "            <key>CFBundleURLName</key>\n            <string>{}</string>\n",
            xml_escape(&bundle_identifier(publisher, product))
        ));
        doc.push_str("            <key>CFBundleURLSchemes</key>\n            <array>\n");
        for scheme in deep_link_schemes {
            doc.push_str(&format!(
                "                <string>{}</string>\n",
                xml_escape(scheme)
            ));
        }
        doc.push_str("            </array>\n        </dict>\n    </array>\n");
    }
    doc.push_str("    <key>LSMinimumSystemVersion</key>\n    <string>10.13</string>\n");
    doc.push_str("</dict>\n</plist>\n");
    doc
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_segments_are_sanitized() {
        assert_eq!(
            bundle_identifier(Some("celestia-island"), "ShunDemo Test"),
            "shun.celestia-island.ShunDemo-Test"
        );
        assert_eq!(bundle_identifier(None, "Widget"), "shun.Widget");
        assert_eq!(bundle_identifier(Some("///"), "App"), "shun.App");
    }

    #[test]
    fn plist_declares_launch_identity() {
        let plist = render_info_plist(
            "Shun & Demo",
            "0.1.0",
            Some("celestia-island"),
            "shun-demo",
            &["shundemo".to_string()],
        );
        assert!(plist.contains("<key>CFBundlePackageType</key>"));
        assert!(plist.contains("<string>APPL</string>"));
        assert!(plist.contains("<string>Shun &amp; Demo</string>"));
        assert!(plist.contains("<key>CFBundleExecutable</key>\n    <string>shun-demo</string>"));
        assert!(plist.contains(
            "<key>CFBundleIdentifier</key>\n    <string>shun.celestia-island.Shun-Demo</string>"
        ));
        assert!(
            plist.contains("<key>CFBundleShortVersionString</key>\n    <string>0.1.0</string>")
        );
        // Deep-link schemes declare themselves through CFBundleURLTypes.
        assert!(plist.contains("<key>CFBundleURLTypes</key>"));
        assert!(plist.contains("<string>shundemo</string>"));
        assert!(plist.ends_with("</dict>\n</plist>\n"));
    }

    #[test]
    fn plist_without_deep_links_omits_url_types() {
        let plist = render_info_plist("Widget", "1.0", None, "widget", &[]);
        assert!(!plist.contains("CFBundleURLTypes"));
    }
}
