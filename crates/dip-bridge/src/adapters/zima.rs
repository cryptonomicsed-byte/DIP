// ZimaOS / CasaOS API adapter for DIP.
//
// UURI scheme: zima://<server-id>/<app-name>
// Config (env vars — no hardcoding):
//   ZIMA_API_URL    — base URL of the ZimaOS/CasaOS API, e.g. http://192.168.1.10:8080
//   ZIMA_API_TOKEN  — Bearer token for authentication
//
// Fail-open: if env vars are not set the adapter is simply not registered;
// the broker continues without ZimaOS support.
//
// Actions mapped to CasaOS v1 API:
//   zima.app.deploy   → POST   {api_url}/v1/apps
//   zima.app.start    → POST   {api_url}/v1/apps/{app}/start
//   zima.app.stop     → POST   {api_url}/v1/apps/{app}/stop
//   zima.app.status   → GET    {api_url}/v1/apps/{app}
//   zima.app.logs     → GET    {api_url}/v1/apps/{app}/logs
//   zima.storage.get  → GET    {api_url}/v1/storage
//   zima.service.list → GET    {api_url}/v1/services

use dip_types::{DipEnvelope, DipMessage};
use serde_json::{json, Value};

/// Zima adapter handle.
///
/// Constructed only when `ZIMA_API_URL` and `ZIMA_API_TOKEN` are both set.
pub struct ZimaAdapter {
    /// Base URL from ZIMA_API_URL env var.
    api_url: String,
    /// Bearer token from ZIMA_API_TOKEN env var.
    api_token: String,
    client: reqwest::Client,
}

impl ZimaAdapter {
    /// Build from environment.  Returns `None` when `ZIMA_API_URL` is unset —
    /// caller should skip registration (fail-open).
    pub fn from_env() -> Option<Self> {
        let api_url = std::env::var("ZIMA_API_URL").ok().filter(|s| !s.is_empty())?;
        // Token is optional; default to empty string (some deployments are open).
        let api_token = std::env::var("ZIMA_API_TOKEN").unwrap_or_default();

        Some(Self {
            api_url: api_url.trim_end_matches('/').to_string(),
            api_token,
            client: reqwest::Client::new(),
        })
    }

    /// URL scheme this adapter handles.
    pub fn scheme(&self) -> &str {
        "zima"
    }

    fn auth_header(&self) -> Option<String> {
        if self.api_token.is_empty() {
            None
        } else {
            Some(format!("Bearer {}", self.api_token))
        }
    }

    /// Route a DipEnvelope to the appropriate ZimaOS/CasaOS API call.
    ///
    /// Expects the envelope payload to be `DipMessage::ToolCall` with:
    ///   tool_name  — one of the zima.* actions above
    ///   arguments  — JSON object with action-specific params
    pub async fn execute(&self, envelope: &DipEnvelope) -> Result<Value, String> {
        let (tool_name, args) = match &envelope.payload {
            DipMessage::ToolCall { tool_name, arguments, .. } => {
                (tool_name.as_str(), arguments.clone())
            }
            _ => {
                return Err(format!(
                    "ZimaAdapter expects ToolCall payload; got other variant (envelope {})",
                    envelope.envelope_id
                ));
            }
        };

        tracing::debug!(
            envelope_id = %envelope.envelope_id,
            tool_name,
            api_url = %self.api_url,
            "ZimaAdapter: routing action"
        );

        match tool_name {
            "zima.app.deploy"   => self.app_deploy(&args).await,
            "zima.app.start"    => self.app_start(app_name(&args)?).await,
            "zima.app.stop"     => self.app_stop(app_name(&args)?).await,
            "zima.app.status"   => self.app_status(app_name(&args)?).await,
            "zima.app.logs"     => self.app_logs(app_name(&args)?).await,
            "zima.storage.get"  => self.storage_get().await,
            "zima.service.list" => self.service_list().await,
            other => Err(format!("ZimaAdapter: unknown action '{}'", other)),
        }
    }

    // ── action implementations ─────────────────────────────────────────────

    async fn app_deploy(&self, args: &Value) -> Result<Value, String> {
        let url = format!("{}/v1/apps", self.api_url);
        let mut req = self.client.post(&url).json(args);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        send(req).await
    }

    async fn app_start(&self, app: &str) -> Result<Value, String> {
        let url = format!("{}/v1/apps/{}/start", self.api_url, app);
        let mut req = self.client.post(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        send(req).await
    }

    async fn app_stop(&self, app: &str) -> Result<Value, String> {
        let url = format!("{}/v1/apps/{}/stop", self.api_url, app);
        let mut req = self.client.post(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        send(req).await
    }

    async fn app_status(&self, app: &str) -> Result<Value, String> {
        let url = format!("{}/v1/apps/{}", self.api_url, app);
        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        send(req).await
    }

    async fn app_logs(&self, app: &str) -> Result<Value, String> {
        let url = format!("{}/v1/apps/{}/logs", self.api_url, app);
        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        send(req).await
    }

    async fn storage_get(&self) -> Result<Value, String> {
        let url = format!("{}/v1/storage", self.api_url);
        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        send(req).await
    }

    async fn service_list(&self) -> Result<Value, String> {
        let url = format!("{}/v1/services", self.api_url);
        let mut req = self.client.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }
        send(req).await
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Extract `app_name` from action arguments.
fn app_name(args: &Value) -> Result<&str, String> {
    args["app_name"]
        .as_str()
        .or_else(|| args["name"].as_str())
        .ok_or_else(|| "ZimaAdapter: argument 'app_name' required".to_string())
}

/// Execute a reqwest RequestBuilder and return the response body as JSON.
async fn send(req: reqwest::RequestBuilder) -> Result<Value, String> {
    let resp = req
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(json!({}));

    if !status.is_success() {
        return Err(format!("ZimaOS API {status}: {body}"));
    }

    Ok(body)
}

// ── DIP wire integration ──────────────────────────────────────────────────────

/// Send a DipEnvelope to ZimaOS and return success/failure as a string error.
/// Called by the AdapterRouter when `to` starts with "zima:".
pub async fn send_envelope(envelope: &DipEnvelope) -> Result<(), String> {
    let adapter = ZimaAdapter::from_env().ok_or_else(|| {
        "ZimaAdapter: ZIMA_API_URL not configured".to_string()
    })?;

    adapter.execute(envelope).await.map(|_| ())
}
