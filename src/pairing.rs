//! The built-in pairing gateway client (the `gateway` lane of the
//! declarative pairing step).
//!
//! Speaks the JSON-RPC device lane proven in evernight's first-run wizard
//! (2026-10): `pairing.request` mints a display code, `pairing.await`
//! long-polls for the operator's acceptance (each call parks up to the
//! gateway's ~20s window — loop from the UI), and the claimed credential
//! travels back with the answer. The pane that renders this lane — big
//! code, countdown, copy — is the shell's prefabricated template; this
//! module is only the wire.
//!
//! Sync by design: the shell calls it from a blocking worker, matching
//! every other delivery-lane client in this crate. Pairing never detours
//! through a configured proxy (a misconfigured system proxy must not break
//! device claiming).

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// One minted display code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingCode {
    /// The 8-character code the operator types into the control panel.
    pub code: String,
    /// Seconds until the code expires (the pane counts these down
    /// locally against a deadline; the gateway re-anchors on each answer).
    pub expires_in: i64,
}

/// The credential delivered when an operator accepts the code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingClaim {
    /// The device id the credential belongs to.
    pub node_id: String,
    /// The device secret — shown once, written once, never logged.
    pub device_secret: String,
    /// The accepting account.
    pub owner: String,
    /// The code that was accepted (echoed for the success card).
    pub pairing_code: String,
}

/// One `pairing.await` answer as the waiting device sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum PairingAwait {
    /// Nobody typed the code yet; seconds left on it.
    Pending {
        /// Seconds until the displayed code expires.
        expires_in: i64,
    },
    /// An operator accepted the code — the credential travels here.
    Claimed {
        #[serde(flatten)]
        claim: PairingClaim,
    },
    /// The code expired or was superseded; mint a fresh one.
    Unknown,
}

/// A pairing gateway endpoint (`.../server` base URL).
#[derive(Debug, Clone)]
pub struct PairingClient {
    /// The gateway base; trailing slashes and stray whitespace are
    /// tolerated the same way evernight's wizard tolerates them.
    base: String,
}

impl PairingClient {
    /// Wraps a gateway base URL (e.g. `https://gateway.example/server`).
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim().trim_end_matches('/').to_string(),
        }
    }

    /// The JSON-RPC endpoint every pairing call posts to.
    pub fn endpoint(&self) -> String {
        format!("{}/api/ws", self.base)
    }

    /// Mints a display code for this device.
    pub fn request(
        &self,
        node_id: &str,
        name: Option<&str>,
        tier: Option<u8>,
    ) -> Result<PairingCode, String> {
        let mut params = serde_json::json!({ "node_id": node_id });
        if let Some(name) = name {
            params["name"] = serde_json::json!(name);
        }
        if let Some(tier) = tier {
            params["tier"] = serde_json::json!(tier);
        }
        let answer = self.post("pairing.request", &params)?;
        let code = answer["code"]
            .as_str()
            .ok_or_else(|| "gateway minted no code".to_string())?
            .to_string();
        let expires_in = answer["expires_in"].as_i64().unwrap_or(300);
        Ok(PairingCode { code, expires_in })
    }

    /// Long-polls the gateway for the outcome of a displayed code. One
    /// call parks up to the gateway's ~20s window; loop from the UI.
    pub fn await_code(&self, node_id: &str, code: &str) -> Result<PairingAwait, String> {
        let answer = self.post(
            "pairing.await",
            &serde_json::json!({ "node_id": node_id, "code": code }),
        )?;
        match answer["status"].as_str().unwrap_or("") {
            "claimed" => {
                let claim = PairingClaim {
                    node_id: answer["node_id"].as_str().unwrap_or(node_id).to_string(),
                    device_secret: answer["device_secret"]
                        .as_str()
                        .ok_or_else(|| "claim carried no device secret".to_string())?
                        .to_string(),
                    owner: answer["owner"].as_str().unwrap_or("").to_string(),
                    pairing_code: answer["pairing_code"].as_str().unwrap_or(code).to_string(),
                };
                Ok(PairingAwait::Claimed { claim })
            }
            "pending" => Ok(PairingAwait::Pending {
                expires_in: answer["expires_in"].as_i64().unwrap_or(0),
            }),
            _ => Ok(PairingAwait::Unknown),
        }
    }

    /// One JSON-RPC POST. Errors carry the gateway's own message when it
    /// answers with one, so the pane can surface the real reason.
    fn post(&self, method: &str, params: &serde_json::Value) -> Result<serde_json::Value, String> {
        // ureq never reads proxy configuration unless one is set
        // explicitly, so pairing cannot detour through a misconfigured
        // system proxy — exactly the guarantee evernight's client built
        // with reqwest's `.no_proxy()`.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(30))
            .build();
        let response = agent
            .post(&self.endpoint())
            .timeout(Duration::from_secs(30))
            .send_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": method, "params": params,
            }))
            .map_err(|e| format!("pairing {method}: {e}"))?;
        let body = response
            .into_string()
            .map_err(|e| format!("pairing {method} body: {e}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("pairing {method} json: {e}"))?;
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("pairing rejected");
            return Err(format!("pairing {method}: {message}"));
        }
        Ok(value.get("result").cloned().unwrap_or(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read as _, Write};
    use std::net::TcpListener;

    /// A one-shot JSON-RPC mock: reads one request, answers with `script`,
    /// returns the request it saw.
    fn mock(script: &'static str) -> (String, std::thread::JoinHandle<serde_json::Value>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap(); // request line
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
                if let Some(value) = header
                    .to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(str::trim)
                {
                    length = value.parse().unwrap();
                }
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let mut stream = stream;
            write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", script.len(), script).unwrap();
            stream.flush().unwrap();
            request
        });
        (format!("http://{addr}/server"), handle)
    }

    #[test]
    fn request_sends_the_device_params_and_parses_the_code() {
        let (base, seen) =
            mock(r#"{"jsonrpc":"2.0","id":1,"result":{"code":"7QK2M4XP","expires_in":300}}"#);
        let client = PairingClient::new(&base);
        let code = client
            .request("node-1", Some("Corridor dryer"), Some(2))
            .unwrap();
        assert_eq!(code.code, "7QK2M4XP");
        assert_eq!(code.expires_in, 300);
        let request = seen.join().unwrap();
        assert_eq!(request["method"], "pairing.request");
        assert_eq!(request["params"]["node_id"], "node-1");
        assert_eq!(request["params"]["name"], "Corridor dryer");
        assert_eq!(request["params"]["tier"], 2);
        assert_eq!(
            request["params"].as_object().unwrap().len(),
            3,
            "no stray params"
        );
        assert_eq!(
            client.endpoint(),
            format!("{}/api/ws", base.trim_end_matches('/')),
            "the endpoint is the /api/ws JSON-RPC route"
        );
    }

    #[test]
    fn request_omits_unset_optionals() {
        let (base, seen) = mock(r#"{"result":{"code":"AAAA","expires_in":60}}"#);
        PairingClient::new(&base)
            .request("node-1", None, None)
            .unwrap();
        let params = seen.join().unwrap()["params"].clone();
        assert_eq!(params.as_object().unwrap().len(), 1, "only node_id is sent");
    }

    #[test]
    fn await_maps_pending_claimed_and_unknown() {
        let (base, _) = mock(r#"{"result":{"status":"pending","expires_in":291}}"#);
        assert_eq!(
            PairingClient::new(&base)
                .await_code("node-1", "7QK2M4XP")
                .unwrap(),
            PairingAwait::Pending { expires_in: 291 }
        );

        let (base, _) = mock(
            r#"{"result":{"status":"claimed","node_id":"node-1","device_secret":"s3cr3t",
                "owner":"ops@Example","pairing_code":"7QK2M4XP"}}"#,
        );
        match PairingClient::new(&base)
            .await_code("node-1", "7QK2M4XP")
            .unwrap()
        {
            PairingAwait::Claimed { claim } => {
                assert_eq!(claim.device_secret, "s3cr3t");
                assert_eq!(claim.owner, "ops@Example");
            }
            other => panic!("expected a claim, got {other:?}"),
        }

        let (base, _) = mock(r#"{"result":{"status":"unknown"}}"#);
        assert_eq!(
            PairingClient::new(&base)
                .await_code("node-1", "dead")
                .unwrap(),
            PairingAwait::Unknown
        );
    }

    #[test]
    fn gateway_errors_surface_their_message() {
        let (base, _) = mock(r#"{"error":{"code":-32001,"message":"rate limited"}}"#);
        let err = PairingClient::new(&base)
            .request("node-1", None, None)
            .unwrap_err();
        assert!(err.contains("rate limited"), "{err}");
    }

    #[test]
    fn a_codeless_answer_is_rejected() {
        let (base, _) = mock(r#"{"result":{"expires_in":300}}"#);
        assert!(
            PairingClient::new(&base)
                .request("node-1", None, None)
                .is_err()
        );
    }
}
