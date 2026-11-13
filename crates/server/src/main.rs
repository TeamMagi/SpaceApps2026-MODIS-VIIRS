use axum::{
    extract::Query,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

use pyrosync_core::models::BBox;
use pyrosync_firms::FirmsClient;

#[derive(Deserialize)]
struct HarmonizeParams {
    west: f64,
    south: f64,
    east: f64,
    north: f64,
    days: Option<u32>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("pyrosync=info".parse()?))
        .init();

    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "web/dist".to_string());

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/harmonize", get(harmonize))
        .fallback_service(ServeDir::new(&static_dir))
        .layer(CorsLayer::permissive());

    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".to_string());
    let addr = format!("0.0.0.0:{port}");
    tracing::info!("PyroSync server listening on {addr}, static files from {static_dir}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn harmonize(
    Query(params): Query<HarmonizeParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let api_key = std::env::var("FIRMS_API_KEY").map_err(|_| {
        tracing::error!("FIRMS_API_KEY not set");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let bbox = BBox {
        west: params.west,
        south: params.south,
        east: params.east,
        north: params.north,
    };

    let days = params.days.unwrap_or(1);
    let client = FirmsClient::new(api_key);

    let (modis, viirs) = client.fetch_all(&bbox, days).await.map_err(|e| {
        tracing::error!("FIRMS fetch failed: {e:#}");
        StatusCode::BAD_GATEWAY
    })?;

    let modis_count = modis.len();
    let viirs_count = viirs.len();

    let harmonized = pyrosync_core::harmonize::harmonize(modis, viirs);
    let calendar = pyrosync_core::calendar::build_calendar(&harmonized);

    Ok(Json(serde_json::json!({
        "raw": {
            "modis_count": modis_count,
            "viirs_count": viirs_count,
            "total_raw": modis_count + viirs_count,
        },
        "harmonized": {
            "count": harmonized.len(),
            "hotspots": harmonized,
        },
        "calendar": calendar,
    })))
}
