//! Update watch — mirror sources probed in order, with the declared
//! files resolved under the first reachable one.
//!
//! The delivery manifest (`[package.metadata.shun.update]`) declares a
//! list of mirror base URLs and a list of file names (`latest` version
//! markers, installer artifacts). [`resolve`] probes the sources in
//! order — a plain GET of the first file — and the first source that
//! answers wins for the whole pass: every declared file then resolves
//! to a URL under the winning base. The shell fetches those URLs
//! itself ([`fetch_text`]) or streams full artifacts through the online
//! payload pipeline ([`OnlinePayload`](crate::payload_online::OnlinePayload)).
//! Requires the `online` feature.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::error::ShunError;
use crate::flow::{FlowEvent, FlowLog, FlowPhase};

/// The module's shared agent: a short timeout on every request, so an
/// unreachable mirror costs seconds instead of hanging the pass. Callers
/// fetching the resolved URLs (version markers, artifacts) reuse it.
pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build()
}

/// One mirror source base plus the files resolved under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWatch {
    /// The winning mirror base URL (trailing slash trimmed).
    pub source: String,
    /// File name → absolute URL under the winning source.
    pub files: BTreeMap<String, String>,
}

/// Probes the sources in order and resolves every declared file under
/// the first reachable one.
///
/// The probe is a plain `GET` of the first file (update markers are
/// tiny); a `200` answer makes the source the winner. Unreachable or
/// non-200 sources are skipped with a [`FlowLog::Warning`] record.
/// Returns `None` when no source answers — or when `sources` or `files`
/// is empty (both are normalized: trailing `/` trimmed, empty entries
/// skipped).
pub fn resolve(
    sources: &[String],
    files: &[String],
    on_event: &mut dyn FnMut(FlowEvent),
) -> Option<ResolvedWatch> {
    let files: Vec<String> = files
        .iter()
        .map(|file| file.trim().to_string())
        .filter(|file| !file.is_empty())
        .collect();
    if files.is_empty() {
        return None;
    }

    let agent = agent();
    for source in sources {
        let base = source.trim().trim_end_matches('/');
        if base.is_empty() {
            continue;
        }

        // The probe fetches the first file — update markers are tiny.
        let probe_url = format!("{base}/{}", files[0]);
        on_event(FlowEvent::Progress {
            phase: FlowPhase::Download,
            step: format!("Checking {}", host_of(base)),
            percent: None,
        });
        match agent.get(&probe_url).call() {
            // ureq hands back 2xx as `Ok`; the probe wins on 200.
            Ok(response) if response.status() == 200 => {
                on_event(FlowEvent::Progress {
                    phase: FlowPhase::Download,
                    step: format!("Using {base} …"),
                    percent: None,
                });
                return Some(ResolvedWatch {
                    source: base.to_string(),
                    files: files
                        .iter()
                        .map(|file| (file.clone(), format!("{base}/{file}")))
                        .collect(),
                });
            }
            Ok(response) => skip(
                on_event,
                base,
                &format!("probe answered {}", response.status()),
            ),
            Err(ureq::Error::Status(status, _)) => {
                skip(on_event, base, &format!("probe answered {status}"));
            }
            Err(err) => skip(on_event, base, &format!("probe failed: {err}")),
        }
    }
    None
}

/// Records one skipped source as a warning — a non-fatal degradation the
/// shell should surface (mirroring the install flow's policy warnings).
fn skip(on_event: &mut dyn FnMut(FlowEvent), base: &str, reason: &str) {
    on_event(FlowEvent::Log {
        record: FlowLog::Warning {
            code: "update-source-skipped".to_string(),
            detail: format!("{base}: {reason}"),
        },
    });
}

/// Fetches a small text file (e.g. a `latest` version marker) as a
/// trimmed string.
pub fn fetch_text(url: &str) -> Result<String, ShunError> {
    let response = agent()
        .get(url)
        .call()
        .map_err(|e| ShunError::Config(format!("fetch {url}: {e}")))?;
    if response.status() != 200 {
        return Err(ShunError::Config(format!(
            "fetch {url}: unexpected status {}",
            response.status()
        )));
    }
    response
        .into_string()
        .map(|text| text.trim().to_string())
        .map_err(|e| ShunError::Config(format!("fetch {url}: {e}")))
}

/// The host of a base URL: whatever sits between `://` and the first
/// `/` (no url crate for one label).
fn host_of(base: &str) -> &str {
    let rest = base.split_once("://").map(|(_, rest)| rest).unwrap_or(base);
    rest.split('/').next().unwrap_or(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_of_extracts_the_host_label() {
        assert_eq!(
            host_of("https://mirror.example.test/files"),
            "mirror.example.test"
        );
        assert_eq!(host_of("http://127.0.0.1:8080"), "127.0.0.1:8080");
        assert_eq!(host_of("127.0.0.1:8080"), "127.0.0.1:8080");
    }
}
