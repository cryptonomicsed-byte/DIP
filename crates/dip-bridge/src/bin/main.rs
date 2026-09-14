use std::sync::Arc;
use axum::{Router, Json, extract::State, routing::{get, post}, http::StatusCode};
use dip_types::{DipEnvelope, AdapterManifest, AdapterKind, AdapterStatus, AdapterCapability};
use dip_bridge::{AdapterRegistry, AdapterRouter};

type AppState = Arc<AppData>;

struct AppData {
    registry: Arc<AdapterRegistry>,
    router:   AdapterRouter,
}

#[tokio::main]
async fn main() {
    let registry = Arc::new(AdapterRegistry::new());
    let router = AdapterRouter::new(registry.clone());
    let state = Arc::new(AppData { registry, router });

    let port: u16 = std::env::var("DIP_PORT")
        .ok().and_then(|p| p.parse().ok())
        .unwrap_or(7792);

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/adapters", get(list_adapters).post(register_adapter))
        .route("/api/dip/outbound", post(route_outbound))
        .route("/api/dip/inbound",  post(receive_inbound))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    println!("DIP bridge listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok", "service": "dip-bridge"}))
}

async fn list_adapters(State(s): State<AppState>) -> Json<Vec<AdapterManifest>> {
    Json(s.registry.list())
}

async fn register_adapter(
    State(s): State<AppState>,
    Json(manifest): Json<AdapterManifest>,
) -> StatusCode {
    s.registry.register(manifest);
    StatusCode::CREATED
}

async fn route_outbound(
    State(s): State<AppState>,
    Json(envelope): Json<DipEnvelope>,
) -> (StatusCode, Json<serde_json::Value>) {
    match s.router.route(envelope).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))),
    }
}

async fn receive_inbound(
    Json(envelope): Json<DipEnvelope>,
) -> Json<serde_json::Value> {
    // Inbound: log + acknowledge. Full dispatch to Vantage/Omo-Koda2 wired in Phase 9.
    println!("DIP inbound: kind={:?} from={} to={}", envelope.kind, envelope.from, envelope.to);
    Json(serde_json::json!({"ok": true, "envelope_id": envelope.envelope_id}))
}
