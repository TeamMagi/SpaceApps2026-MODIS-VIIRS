use std::sync::Arc;

use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

use pyrosync_core::models::{BBox, Sensor};
use pyrosync_firms::{DayRange, FirmsClient, FirmsSource};

#[derive(Clone)]
struct AppState {
    firms: Option<Arc<FirmsClient>>,
}

#[derive(Deserialize)]
struct HarmonizeParams {
    west: f64,
    south: f64,
    east: f64,
    north: f64,
    days: Option<u32>,
    start: Option<String>,
    end: Option<String>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct AvailabilityEntry {
    source: &'static str,
    min_date: String,
    max_date: String,
}

#[derive(Serialize)]
struct ReportResponse {
    per_source: Vec<(String, usize)>,
    failures: Vec<(String, String)>,
    transactions_spent: u32,
    uncovered: Vec<(String, String)>,
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

enum TimeWindow {
    Days(DayRange),
    Range { start: NaiveDate, end: NaiveDate },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("pyrosync=info".parse()?))
        .init();

    // Keep the server bootable without an API key so health checks and static assets stay live.
    let firms = match std::env::var("FIRMS_API_KEY") {
        Ok(key) if !key.trim().is_empty() => match FirmsClient::new(key) {
            Ok(client) => Some(Arc::new(client)),
            Err(err) => {
                tracing::error!("Failed to initialize FIRMS client: {err}");
                None
            }
        },
        _ => {
            tracing::warn!("FIRMS_API_KEY is not configured; FIRMS endpoints will return 500");
            None
        }
    };

    let state = AppState { firms };
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "web/dist".to_string());
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/harmonize", get(harmonize))
        .route("/api/availability", get(availability))
        .with_state(state)
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

async fn availability(
    State(state): State<AppState>,
) -> Result<Json<Vec<AvailabilityEntry>>, ApiError> {
    let client = require_firms_client(&state)?;
    let avail = client
        .availability()
        .await
        .map_err(|e| ApiError::bad_gateway(format!("Failed to fetch FIRMS availability: {e}")))?;

    // MODIS Terra and Aqua share the same FIRMS source IDs, so probe Terra only to avoid duplicates.
    const SENSORS: [Sensor; 4] = [
        Sensor::ModisTerra,
        Sensor::ViirsSNPP,
        Sensor::ViirsNOAA20,
        Sensor::ViirsNOAA21,
    ];

    let mut entries = Vec::new();
    for sensor in SENSORS {
        for source in FirmsSource::all_for(sensor) {
            if let Some((min_date, max_date)) = avail.range_for(source) {
                entries.push(AvailabilityEntry {
                    source: source.as_str(),
                    min_date: min_date.format("%Y-%m-%d").to_string(),
                    max_date: max_date.format("%Y-%m-%d").to_string(),
                });
            }
        }
    }
    entries.sort_by_key(|entry| entry.source);

    Ok(Json(entries))
}

async fn harmonize(
    State(state): State<AppState>,
    Query(params): Query<HarmonizeParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let bbox = parse_bbox(&params)?;
    let window = parse_time_window(&params)?;
    let client = require_firms_client(&state)?;

    let (mut modis, mut viirs, report) = match window {
        TimeWindow::Days(days) => client.fetch_all(&bbox, days).await,
        TimeWindow::Range { start, end } => client.fetch_all_range(&bbox, start, end).await,
    };

    if modis.is_empty() && viirs.is_empty() && !report.failures.is_empty() {
        let details = report
            .failures
            .iter()
            .map(|(source, err)| format!("{source}: {err}"))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ApiError::bad_gateway(format!(
            "Upstream FIRMS request failed: {details}"
        )));
    }

    pyrosync_firms::parse::retain_wildfires(&mut modis);
    pyrosync_firms::parse::retain_wildfires(&mut viirs);

    let modis_count = modis.len();
    let viirs_count = viirs.len();
    let harmonized = pyrosync_core::harmonize::harmonize(modis, viirs);
    let calendar = pyrosync_core::calendar::build_calendar(&harmonized);

    let report_json = ReportResponse {
        per_source: report.per_source,
        failures: report.failures,
        transactions_spent: report.transactions_spent,
        uncovered: report
            .uncovered
            .into_iter()
            .map(|(start, end)| {
                (
                    start.format("%Y-%m-%d").to_string(),
                    end.format("%Y-%m-%d").to_string(),
                )
            })
            .collect(),
    };

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
        "report": report_json,
    })))
}

fn require_firms_client(state: &AppState) -> Result<&FirmsClient, ApiError> {
    state
        .firms
        .as_deref()
        .ok_or_else(|| ApiError::internal("FIRMS_API_KEY is not configured"))
}

fn parse_bbox(params: &HarmonizeParams) -> Result<BBox, ApiError> {
    if !(-180.0..=180.0).contains(&params.west) || !(-180.0..=180.0).contains(&params.east) {
        return Err(ApiError::bad_request(
            "Longitude coordinates ('west', 'east') must be within [-180, 180]",
        ));
    }
    if !(-90.0..=90.0).contains(&params.south) || !(-90.0..=90.0).contains(&params.north) {
        return Err(ApiError::bad_request(
            "Latitude coordinates ('south', 'north') must be within [-90, 90]",
        ));
    }
    if params.west > params.east {
        return Err(ApiError::bad_request(
            "'west' must be less than or equal to 'east'",
        ));
    }
    if params.south > params.north {
        return Err(ApiError::bad_request(
            "'south' must be less than or equal to 'north'",
        ));
    }

    Ok(BBox {
        west: params.west,
        south: params.south,
        east: params.east,
        north: params.north,
    })
}

fn parse_time_window(params: &HarmonizeParams) -> Result<TimeWindow, ApiError> {
    match (params.days, params.start.as_deref(), params.end.as_deref()) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => Err(ApiError::bad_request(
            "'days' cannot be combined with 'start' or 'end'",
        )),
        (None, Some(_), None) | (None, None, Some(_)) => Err(ApiError::bad_request(
            "Both 'start' and 'end' are required when specifying a date range",
        )),
        (None, Some(start_str), Some(end_str)) => {
            let start = NaiveDate::parse_from_str(start_str, "%Y-%m-%d").map_err(|_| {
                ApiError::bad_request(format!(
                    "Invalid 'start' date '{start_str}': expected YYYY-MM-DD"
                ))
            })?;
            let end = NaiveDate::parse_from_str(end_str, "%Y-%m-%d").map_err(|_| {
                ApiError::bad_request(format!(
                    "Invalid 'end' date '{end_str}': expected YYYY-MM-DD"
                ))
            })?;
            if end < start {
                return Err(ApiError::bad_request(format!(
                    "'end' date ({end}) must not be earlier than 'start' date ({start})"
                )));
            }
            Ok(TimeWindow::Range { start, end })
        }
        (days_opt, None, None) => {
            let days = days_opt.unwrap_or(1);
            let day_range = u8::try_from(days)
                .ok()
                .and_then(|d| DayRange::new(d).ok())
                .ok_or_else(|| {
                    ApiError::bad_request(format!(
                        "Invalid 'days' value {days}: must be in the range 1..=5"
                    ))
                })?;
            Ok(TimeWindow::Days(day_range))
        }
    }
}
