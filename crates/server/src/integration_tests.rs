//! End-to-end integration tests: the real router and `FirmsClient` run against a wiremock FIRMS
//! server, while the calendar runs against an offline archive directory.

use super::*;

use std::path::Path;

use axum::body::Body;
use axum::http::Request;
use chrono::Datelike;
use rstest::rstest;
use serde_json::{Value, json};
use tower::ServiceExt;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "e2e-test-map-key-5f3c";
const BBOX: &str = "west=26&south=36&east=45&north=42";
const MODIS_HEADER: &str = "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight";
const VIIRS_HEADER: &str = "latitude,longitude,bright_ti4,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_ti5,frp,daynight";

fn today() -> NaiveDate {
    Utc::now().date_naive()
}

fn ymd(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

// VIIRS rows 1-2 are an interscan duplicate (same time, ~14 m apart). The merged VIIRS point
// matches MODIS row 1 (0.14 km and 60 min apart), while MODIS row 2 and VIIRS row 3 have no
// partner, so 5 raw detections harmonize into 3 records: Matched, ModisOnly and ViirsOnly.
fn modis_csv(date: &str) -> String {
    format!(
        "{MODIS_HEADER}\n\
         39.0000,32.0000,330.5,1.0,1.0,{date},1000,Terra,MODIS,80,6.1NRT,295.0,50.0,D\n\
         40.0000,33.0000,325.0,1.0,1.0,{date},1030,Aqua,MODIS,60,6.1NRT,293.0,30.0,D\n"
    )
}

fn viirs_snpp_csv(date: &str) -> String {
    format!(
        "{VIIRS_HEADER}\n\
         39.0010,32.0010,340.0,0.4,0.4,{date},1100,N,VIIRS,h,2.0NRT,290.0,40.0,D\n\
         39.0011,32.0011,338.0,0.4,0.4,{date},1100,N,VIIRS,n,2.0NRT,289.0,20.0,D\n\
         41.0000,34.0000,310.0,0.4,0.4,{date},1100,N,VIIRS,l,2.0NRT,285.0,10.0,D\n"
    )
}

fn availability_csv(today: NaiveDate) -> String {
    let min = ymd(today - chrono::Duration::days(60));
    let max = ymd(today + chrono::Duration::days(2));
    format!(
        "data_id,min_date,max_date\n\
         MODIS_NRT,{min},{max}\n\
         VIIRS_SNPP_NRT,{min},{max}\n\
         VIIRS_NOAA20_NRT,{min},{max}\n\
         VIIRS_NOAA21_NRT,{min},{max}\n"
    )
}

async fn mount_area(server: &MockServer, source: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path_regex(format!("^/api/area/csv/{KEY}/{source}/")))
        .respond_with(response)
        .mount(server)
        .await;
}

async fn healthy_firms() -> MockServer {
    let server = MockServer::start().await;
    let date = today();
    let date_str = ymd(date);

    Mock::given(method("GET"))
        .and(path(format!("/api/data_availability/csv/{KEY}/ALL")))
        .respond_with(ResponseTemplate::new(200).set_body_string(availability_csv(date)))
        .mount(&server)
        .await;

    mount_area(
        &server,
        "MODIS_NRT",
        ResponseTemplate::new(200).set_body_string(modis_csv(&date_str)),
    )
    .await;
    mount_area(
        &server,
        "VIIRS_SNPP_NRT",
        ResponseTemplate::new(200).set_body_string(viirs_snpp_csv(&date_str)),
    )
    .await;
    mount_area(
        &server,
        "VIIRS_NOAA20_NRT",
        ResponseTemplate::new(200).set_body_string(format!("{VIIRS_HEADER}\n")),
    )
    .await;
    mount_area(
        &server,
        "VIIRS_NOAA21_NRT",
        ResponseTemplate::new(200).set_body_string(format!("{VIIRS_HEADER}\n")),
    )
    .await;

    server
}

fn firms_client(server: &MockServer) -> FirmsClient {
    FirmsClient::new(KEY.to_string())
        .expect("client builds")
        .with_api_root(server.uri())
}

fn app(
    firms: Option<FirmsClient>,
    archive_dir: Option<&Path>,
    request_timeout: Option<Duration>,
) -> Router {
    let state = AppState {
        firms: firms.map(Arc::new),
        archive: archive_dir.map(|dir| Arc::new(ArchiveClient::offline(dir.to_path_buf()))),
        cache: Arc::new(RwLock::new(ResponseCache::new(8 * 1024 * 1024))),
        request_timeout,
    };
    build_router(
        state,
        None,
        "nonexistent-static-dir-for-e2e",
        build_cors_layer(None),
    )
}

struct Reply {
    status: StatusCode,
    x_cache: Option<String>,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|err| {
            panic!("expected a JSON response body ({err}); got: {}", self.body)
        })
    }
}

async fn call(app: &Router, uri: &str) -> Reply {
    let request = Request::get(uri)
        .body(Body::empty())
        .expect("build request");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("router should respond");

    let status = response.status();
    let x_cache = response
        .headers()
        .get("x-cache")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");

    Reply {
        status,
        x_cache,
        body: String::from_utf8(body.to_vec()).expect("utf-8 response body"),
    }
}

async fn upstream_paths(server: &MockServer) -> Vec<String> {
    let mut paths: Vec<String> = server
        .received_requests()
        .await
        .expect("wiremock server is running")
        .iter()
        .map(|request| request.url.path().to_string())
        .collect();
    paths.sort();
    paths
}

async fn area_requests(server: &MockServer, source: &str) -> usize {
    let prefix = format!("/api/area/csv/{KEY}/{source}/");
    server
        .received_requests()
        .await
        .expect("wiremock server is running")
        .iter()
        .filter(|request| request.url.path().starts_with(&prefix))
        .count()
}

fn error_text(reply: &Reply) -> String {
    reply.json()["error"]
        .as_str()
        .expect("error field should be a string")
        .to_string()
}

const ARCHIVE_HEADER: &str = "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight,type";

fn archive_row(date: &str, lat: f64, fire_type: u8) -> String {
    format!(
        "{lat:.4},30.0000,320.0,1.0,1.0,{date},1000,Terra,MODIS,80,6.1,290.0,25.0,D,{fire_type}"
    )
}

fn write_archive_year(root: &Path, year: i32, rows: &[String]) {
    let year_dir = root.join("modis").join(year.to_string());
    let file = year_dir.join(format!("modis_{year}_Testland.csv"));
    std::fs::create_dir_all(year_dir).expect("create archive year directory");

    let mut body = String::from(ARCHIVE_HEADER);
    body.push('\n');
    for row in rows {
        body.push_str(row);
        body.push('\n');
    }

    std::fs::write(file, body).expect("write archive csv");
}

// Every row is Terra at 10:00 on a different day, so dedup, fleet merging and matching never merge
// them, and this MODIS-only series has an effort factor of exactly 1.0. July counts 2, 4, 6 give
// population z-scores -1.224744871391589, 0, +1.224744871391589; August 2010 is the only August,
// so its anomaly is null.
fn seed_archive(root: &Path) {
    write_archive_year(
        root,
        2008,
        &[
            archive_row("2008-07-01", 37.0, 0),
            archive_row("2008-07-02", 37.1, 0),
        ],
    );

    let rows_2009: Vec<String> = (1..=4u32)
        .map(|day| {
            archive_row(
                &format!("2009-07-{day:02}"),
                37.0 + f64::from(day - 1) * 0.1,
                0,
            )
        })
        .collect();
    write_archive_year(root, 2009, &rows_2009);

    let mut rows_2010: Vec<String> = (1..=6u32)
        .map(|day| {
            archive_row(
                &format!("2010-07-{day:02}"),
                37.0 + f64::from(day - 1) * 0.1,
                0,
            )
        })
        .collect();
    rows_2010.push(archive_row("2010-07-07", 37.0, 2));
    rows_2010.push(archive_row("2010-08-01", 37.5, 0));
    write_archive_year(root, 2010, &rows_2010);
}

#[tokio::test]
async fn health_returns_ok_and_crate_version() {
    let app = app(None, None, None);
    let reply = call(&app, "/api/health").await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);

    let v = reply.json();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn harmonize_end_to_end_against_mock_firms() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let uri = format!("/api/harmonize?{BBOX}&days=1&include_raw=true");
    let reply = call(&app, &uri).await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);
    assert_eq!(
        reply.x_cache.as_deref(),
        Some("MISS"),
        "body: {}",
        reply.body
    );

    let v = reply.json();
    assert_eq!(v["raw"]["modis_count"], 2);
    assert_eq!(v["raw"]["viirs_count"], 3);
    assert_eq!(v["raw"]["total_raw"], 5);
    assert_eq!(
        v["raw"]["hotspots"].as_array().map(|items| items.len()),
        Some(5)
    );

    let count = v["harmonized"]["count"].as_u64().expect("harmonized count");
    assert!(count > 0, "harmonized count should be positive: {count}");
    assert_eq!(count, 3);
    let hotspots = v["harmonized"]["hotspots"]
        .as_array()
        .expect("hotspots array");
    assert_eq!(hotspots.len(), 3);
    let mut sources: Vec<&str> = hotspots
        .iter()
        .map(|hotspot| hotspot["source"].as_str().expect("source string"))
        .collect();
    sources.sort_unstable();
    assert_eq!(sources, vec!["Matched", "ModisOnly", "ViirsOnly"]);

    let calendar = v["calendar"].as_array().expect("calendar array");
    assert_eq!(calendar.len(), 1);
    assert_eq!(calendar[0]["year"], today().year());
    assert_eq!(calendar[0]["month"], today().month());
    assert_eq!(calendar[0]["count"], 3);

    assert_eq!(
        v["report"]["per_source"],
        json!([
            ["MODIS_NRT", 2],
            ["VIIRS_NOAA20_NRT", 0],
            ["VIIRS_NOAA21_NRT", 0],
            ["VIIRS_SNPP_NRT", 3]
        ])
    );
    assert_eq!(v["report"]["failures"], json!([]));
    assert_eq!(v["report"]["uncovered"], json!([]));
    assert_eq!(v["report"]["transactions_spent"], 4);
    assert_eq!(v["warnings"], json!([]));
    assert_eq!(v["effort"]["reliable"].as_bool(), Some(true));

    assert!(
        !reply.body.contains(KEY),
        "body leaked the API key: {}",
        reply.body
    );

    let date = ymd(today());
    let mut expected = vec![format!("/api/data_availability/csv/{KEY}/ALL")];
    for source in [
        "MODIS_NRT",
        "VIIRS_SNPP_NRT",
        "VIIRS_NOAA20_NRT",
        "VIIRS_NOAA21_NRT",
    ] {
        expected.push(format!("/api/area/csv/{KEY}/{source}/26,36,45,42/1/{date}"));
    }
    expected.sort();
    assert_eq!(upstream_paths(&firms).await, expected);
}

#[tokio::test]
async fn compare_shows_fewer_harmonized_records_than_raw_detections() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let reply = call(&app, &format!("/api/compare?{BBOX}&days=1")).await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);

    let v = reply.json();
    let c = &v["counts"];
    assert_eq!(c["raw_total"], 5);
    assert_eq!(c["raw_modis"], 2);
    assert_eq!(c["raw_viirs"], 3);
    assert_eq!(c["harmonized_total"], 3);
    assert_eq!(c["removed_total"], 2);
    let removed_pct = c["removed_pct"].as_f64().expect("removed_pct");
    assert!(
        (removed_pct - 40.0).abs() < 1e-9,
        "removed_pct {removed_pct}"
    );
    assert_eq!(
        c["by_source"],
        json!({"modis_only": 1, "viirs_only": 1, "matched": 1})
    );

    let raw_total = c["raw_total"].as_u64().expect("raw_total");
    let harmonized_total = c["harmonized_total"].as_u64().expect("harmonized_total");
    assert!(
        raw_total > harmonized_total,
        "raw {raw_total} should exceed harmonized {harmonized_total}"
    );
}

#[tokio::test]
async fn stats_summarises_the_same_pipeline_run() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let reply = call(&app, &format!("/api/stats?{BBOX}&days=1")).await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);

    let v = reply.json();
    assert_eq!(v["totals"]["raw_detections"], 5);
    assert_eq!(v["totals"]["harmonized_records"], 3);
    assert_eq!(v["totals"]["calendar_months"], 1);
    assert_eq!(v["totals"]["distinct_platforms"], 3);
    assert_eq!(v["pipeline"]["transactions_spent"], 4);
    assert_eq!(v["pipeline"]["sources_queried"], 4);
    assert_eq!(v["pipeline"]["sources_failed"], 0);
}

#[tokio::test]
async fn availability_lists_mock_sources_sorted() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let reply = call(&app, "/api/availability").await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);

    let v = reply.json();
    let entries = v.as_array().expect("availability array");
    let sources: Vec<&str> = entries
        .iter()
        .map(|entry| entry["source"].as_str().expect("source string"))
        .collect();
    assert_eq!(
        sources,
        vec![
            "MODIS_NRT",
            "VIIRS_NOAA20_NRT",
            "VIIRS_NOAA21_NRT",
            "VIIRS_SNPP_NRT"
        ]
    );

    let min = ymd(today() - chrono::Duration::days(60));
    let max = ymd(today() + chrono::Duration::days(2));
    for entry in entries {
        assert_eq!(entry["min_date"].as_str(), Some(min.as_str()));
        assert_eq!(entry["max_date"].as_str(), Some(max.as_str()));
    }
}

#[tokio::test]
async fn identical_request_is_served_from_cache_without_new_upstream_calls() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );
    let uri = format!("/api/harmonize?{BBOX}&days=1");

    let first = call(&app, &uri).await;
    assert_eq!(first.status, StatusCode::OK, "body: {}", first.body);
    assert_eq!(first.x_cache.as_deref(), Some("MISS"));
    assert_eq!(upstream_paths(&firms).await.len(), 5);

    let second = call(&app, &uri).await;
    assert_eq!(second.status, StatusCode::OK, "body: {}", second.body);
    assert_eq!(second.x_cache.as_deref(), Some("HIT"));
    assert_eq!(second.body, first.body);
    assert_eq!(upstream_paths(&firms).await.len(), 5);

    let with_raw = call(
        &app,
        &format!("/api/harmonize?{BBOX}&days=1&include_raw=true"),
    )
    .await;
    assert_eq!(with_raw.status, StatusCode::OK, "body: {}", with_raw.body);
    assert_eq!(with_raw.x_cache.as_deref(), Some("MISS"));
    // The availability table is fetched once per client and reused, so only the 4 area requests repeat.
    assert_eq!(upstream_paths(&firms).await.len(), 9);
}

#[tokio::test]
async fn range_outside_availability_is_reported_as_uncovered() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let reply = call(
        &app,
        &format!("/api/harmonize?{BBOX}&start=2020-01-01&end=2020-01-03"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);

    let v = reply.json();
    assert_eq!(v["harmonized"]["count"], 0);
    assert_eq!(
        v["report"]["uncovered"],
        json!([["2020-01-01", "2020-01-03"]])
    );
    let warnings = v["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|warning| warning
            .as_str()
            .unwrap_or_default()
            .contains("not covered by any FIRMS source")),
        "warnings: {warnings:?}"
    );
    assert_eq!(
        upstream_paths(&firms).await,
        vec![format!("/api/data_availability/csv/{KEY}/ALL")]
    );
}

#[tokio::test]
async fn rejected_key_body_returns_502_without_leaking_the_key() {
    let firms = healthy_firms().await;
    Mock::given(method("GET"))
        .and(path_regex(format!("^/api/area/csv/{KEY}/")))
        .respond_with(ResponseTemplate::new(200).set_body_string("Invalid MAP_KEY."))
        .with_priority(1)
        .mount(&firms)
        .await;

    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );
    let reply = call(&app, &format!("/api/harmonize?{BBOX}&days=1")).await;
    assert_eq!(
        reply.status,
        StatusCode::BAD_GATEWAY,
        "body: {}",
        reply.body
    );

    let text = error_text(&reply);
    assert!(
        text.starts_with("Upstream FIRMS request failed:"),
        "error: {text}"
    );
    assert!(text.contains("Invalid MAP_KEY"), "error: {text}");
    assert!(
        !reply.body.contains(KEY),
        "body leaked the API key: {}",
        reply.body
    );
    assert!(
        reply.body.contains("***"),
        "body should redact the key: {}",
        reply.body
    );

    for source in [
        "MODIS_NRT",
        "VIIRS_SNPP_NRT",
        "VIIRS_NOAA20_NRT",
        "VIIRS_NOAA21_NRT",
    ] {
        assert_eq!(area_requests(&firms, source).await, 1, "source {source}");
    }
}

#[tokio::test]
async fn one_failing_source_degrades_to_200_with_warning() {
    let firms = healthy_firms().await;
    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/api/area/csv/{KEY}/VIIRS_NOAA20_NRT/"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string("Invalid MAP_KEY."))
        .with_priority(1)
        .mount(&firms)
        .await;

    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );
    let reply = call(&app, &format!("/api/harmonize?{BBOX}&days=1")).await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);

    let v = reply.json();
    assert_eq!(v["harmonized"]["count"], 3);
    let failures = v["report"]["failures"].as_array().expect("failures array");
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0][0], "VIIRS_NOAA20_NRT");

    let warnings = v["warnings"].as_array().expect("warnings array");
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0]
            .as_str()
            .unwrap_or_default()
            .contains("1 FIRMS request failed"),
        "warnings: {warnings:?}"
    );
    assert_eq!(v["effort"]["reliable"].as_bool(), Some(false));
}

#[tokio::test]
async fn availability_failure_maps_to_502() {
    let firms = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/data_availability/csv/{KEY}/ALL")))
        .respond_with(ResponseTemplate::new(200).set_body_string("Invalid MAP_KEY."))
        .mount(&firms)
        .await;

    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let availability = call(&app, "/api/availability").await;
    assert_eq!(
        availability.status,
        StatusCode::BAD_GATEWAY,
        "body: {}",
        availability.body
    );
    let text = error_text(&availability);
    assert!(
        text.starts_with("Failed to fetch FIRMS availability"),
        "error: {text}"
    );
    assert!(
        !availability.body.contains(KEY),
        "body leaked the API key: {}",
        availability.body
    );

    let harmonize = call(&app, &format!("/api/harmonize?{BBOX}&days=1")).await;
    assert_eq!(
        harmonize.status,
        StatusCode::BAD_GATEWAY,
        "body: {}",
        harmonize.body
    );
    assert!(
        error_text(&harmonize).contains("availability"),
        "error: {}",
        harmonize.body
    );
    assert!(
        !harmonize.body.contains(KEY),
        "body leaked the API key: {}",
        harmonize.body
    );
}

#[tokio::test]
async fn transient_503_is_retried_and_both_attempts_are_counted() {
    let firms = healthy_firms().await;
    Mock::given(method("GET"))
        .and(path_regex(format!("^/api/area/csv/{KEY}/MODIS_NRT/")))
        .respond_with(ResponseTemplate::new(503).insert_header("Retry-After", "0"))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&firms)
        .await;

    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );
    let reply = call(&app, &format!("/api/harmonize?{BBOX}&days=1")).await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);

    let v = reply.json();
    assert_eq!(v["raw"]["modis_count"], 2);
    assert_eq!(v["report"]["failures"], json!([]));
    assert_eq!(v["report"]["transactions_spent"], 5);
    assert_eq!(area_requests(&firms, "MODIS_NRT").await, 2);
}

#[tokio::test]
async fn slow_upstream_hits_request_timeout_with_504() {
    let firms = healthy_firms().await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/area/csv/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!("{VIIRS_HEADER}\n"))
                .set_delay(Duration::from_secs(5)),
        )
        .with_priority(1)
        .mount(&firms)
        .await;

    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(1)),
    );
    let reply = call(&app, &format!("/api/harmonize?{BBOX}&days=1")).await;
    assert_eq!(
        reply.status,
        StatusCode::GATEWAY_TIMEOUT,
        "body: {}",
        reply.body
    );
    assert!(
        error_text(&reply).contains("timed out"),
        "error: {}",
        reply.body
    );
}

#[rstest]
#[case::harmonize("/api/harmonize?west=26&south=36&east=45&north=42&days=1")]
#[case::compare("/api/compare?west=26&south=36&east=45&north=42&days=1")]
#[case::stats("/api/stats?west=26&south=36&east=45&north=42&days=1")]
#[case::availability("/api/availability")]
#[tokio::test]
async fn firms_endpoints_without_api_key_return_500_with_reason(#[case] uri: &str) {
    let app = app(None, None, None);
    let reply = call(&app, uri).await;
    assert_eq!(
        reply.status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "uri {uri} body: {}",
        reply.body
    );
    assert_eq!(error_text(&reply), "FIRMS_API_KEY is not configured");
}

#[rstest]
// days: valid class 1..=5
#[case::days_min("west=26&south=36&east=45&north=42&days=1", StatusCode::OK)]
#[case::days_max("west=26&south=36&east=45&north=42&days=5", StatusCode::OK)]
#[case::days_zero("west=26&south=36&east=45&north=42&days=0", StatusCode::BAD_REQUEST)]
#[case::days_six("west=26&south=36&east=45&north=42&days=6", StatusCode::BAD_REQUEST)]
#[case::days_overflows_u32(
    "west=26&south=36&east=45&north=42&days=4294967296",
    StatusCode::BAD_REQUEST
)]
#[case::days_negative("west=26&south=36&east=45&north=42&days=-1", StatusCode::BAD_REQUEST)]
#[case::days_not_a_number("west=26&south=36&east=45&north=42&days=abc", StatusCode::BAD_REQUEST)]
// bbox: latitude within [-90, 90], longitude within [-180, 180], west <= east, south <= north
#[case::north_at_pole("west=26&south=36&east=45&north=90&days=1", StatusCode::OK)]
#[case::north_past_pole(
    "west=26&south=36&east=45&north=90.0001&days=1",
    StatusCode::BAD_REQUEST
)]
#[case::south_at_pole("west=26&south=-90&east=45&north=42&days=1", StatusCode::OK)]
#[case::south_past_pole(
    "west=26&south=-90.0001&east=45&north=42&days=1",
    StatusCode::BAD_REQUEST
)]
#[case::west_at_antimeridian("west=-180&south=36&east=45&north=42&days=1", StatusCode::OK)]
#[case::west_past_antimeridian(
    "west=-180.0001&south=36&east=45&north=42&days=1",
    StatusCode::BAD_REQUEST
)]
#[case::east_at_antimeridian("west=26&south=36&east=180&north=42&days=1", StatusCode::OK)]
#[case::east_past_antimeridian(
    "west=26&south=36&east=180.0001&north=42&days=1",
    StatusCode::BAD_REQUEST
)]
#[case::west_equals_east("west=30&south=36&east=30&north=42&days=1", StatusCode::OK)]
#[case::west_greater_than_east("west=46&south=36&east=45&north=42&days=1", StatusCode::BAD_REQUEST)]
#[case::south_greater_than_north(
    "west=26&south=43&east=45&north=42&days=1",
    StatusCode::BAD_REQUEST
)]
#[case::west_not_a_number("west=NaN&south=36&east=45&north=42&days=1", StatusCode::BAD_REQUEST)]
// Today this body is plain text, not JSON (BUG-2), so only the status is asserted.
#[case::south_missing("west=26&east=45&north=42&days=1", StatusCode::BAD_REQUEST)]
// time window: days XOR (start AND end)
#[case::no_time_params_default_to_one_day("west=26&south=36&east=45&north=42", StatusCode::OK)]
#[case::days_with_start(
    "west=26&south=36&east=45&north=42&days=1&start={today}",
    StatusCode::BAD_REQUEST
)]
#[case::days_with_end(
    "west=26&south=36&east=45&north=42&days=1&end={today}",
    StatusCode::BAD_REQUEST
)]
#[case::start_without_end(
    "west=26&south=36&east=45&north=42&start={today}",
    StatusCode::BAD_REQUEST
)]
#[case::end_without_start(
    "west=26&south=36&east=45&north=42&end={today}",
    StatusCode::BAD_REQUEST
)]
#[case::start_bad_format(
    "west=26&south=36&east=45&north=42&start=2026/01/01&end={today}",
    StatusCode::BAD_REQUEST
)]
#[case::end_bad_format(
    "west=26&south=36&east=45&north=42&start={today}&end=2026-13-01",
    StatusCode::BAD_REQUEST
)]
#[case::end_before_start(
    "west=26&south=36&east=45&north=42&start={today}&end={yesterday}",
    StatusCode::BAD_REQUEST
)]
#[case::single_day_range(
    "west=26&south=36&east=45&north=42&start={today}&end={today}",
    StatusCode::OK
)]
#[tokio::test]
async fn harmonize_query_validation(#[case] query: &str, #[case] expected: StatusCode) {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let query = query
        .replace("{today}", &ymd(today()))
        .replace("{yesterday}", &ymd(today() - chrono::Duration::days(1)));
    let reply = call(&app, &format!("/api/harmonize?{query}")).await;
    assert_eq!(reply.status, expected, "query {query} body: {}", reply.body);

    if expected == StatusCode::BAD_REQUEST {
        let paths = upstream_paths(&firms).await;
        assert!(
            paths.is_empty(),
            "query {query} should not reach FIRMS, got {paths:?}"
        );
    } else {
        let count = reply.json()["harmonized"]["count"].as_u64().unwrap_or(0);
        assert!(
            count > 0,
            "query {query} should return records, body: {}",
            reply.body
        );
    }
}

#[tokio::test]
async fn calendar_from_offline_archive_is_sorted_with_population_z_scores() {
    let dir = tempfile::tempdir().expect("temp dir");
    seed_archive(dir.path());
    let app = app(None, Some(dir.path()), Some(Duration::from_secs(30)));

    let uri = "/api/calendar?country=Testland&from=2008&to=2010";
    let reply = call(&app, uri).await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);
    assert_eq!(reply.x_cache.as_deref(), Some("MISS"));

    let v = reply.json();
    let cells = v["calendar"].as_array().expect("calendar array");
    let keys: Vec<(i64, i64)> = cells
        .iter()
        .map(|cell| {
            (
                cell["year"].as_i64().expect("year"),
                cell["month"].as_i64().expect("month"),
            )
        })
        .collect();
    assert_eq!(keys, vec![(2008, 7), (2009, 7), (2010, 7), (2010, 8)]);

    let counts: Vec<u64> = cells
        .iter()
        .map(|cell| cell["count"].as_u64().expect("count"))
        .collect();
    assert_eq!(counts, vec![2, 4, 6, 1]);

    let expected_anomalies = [-1.224744871391589_f64, 0.0, 1.224744871391589];
    for (cell, expected) in cells.iter().take(3).zip(expected_anomalies) {
        let anomaly = cell["anomaly"].as_f64().expect("anomaly should be present");
        assert!(
            (anomaly - expected).abs() < 1e-9,
            "anomaly {anomaly} != {expected}"
        );
    }
    assert!(
        cells[3]["anomaly"].is_null(),
        "august anomaly should be null: {:?}",
        cells[3]["anomaly"]
    );

    for (cell, count) in cells.iter().zip(&counts) {
        let omega = cell["effort_omega"]
            .as_f64()
            .expect("effort_omega should be present");
        assert!((omega - 1.0).abs() < 1e-12, "omega {omega}");
        let normalized = cell["count_normalized"]
            .as_f64()
            .expect("count_normalized should be present");
        assert!(
            (normalized - *count as f64).abs() < 1e-12,
            "normalized {normalized}"
        );
    }

    assert_eq!(v["counts"]["raw_detections"], 13);
    assert_eq!(v["counts"]["harmonized_records"], 13);
    assert_eq!(v["fetch"]["failures"], json!([]));

    let detections: Vec<u64> = v["fetch"]["per_sensor_year"]
        .as_array()
        .expect("per_sensor_year array")
        .iter()
        .map(|entry| entry["detections"].as_u64().expect("detections"))
        .collect();
    assert_eq!(detections, vec![2, 4, 7]);

    assert_eq!(v["years"]["effective_from"], 2008);
    assert_eq!(v["years"]["effective_to"], 2010);
    assert_eq!(v["effort"]["reliable"].as_bool(), Some(true));
    let warnings = v["warnings"].as_array().expect("warnings array");
    assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");

    let second = call(&app, uri).await;
    assert_eq!(second.status, StatusCode::OK, "body: {}", second.body);
    assert_eq!(second.x_cache.as_deref(), Some("HIT"));
}

#[tokio::test]
async fn calendar_reports_missing_archive_year_but_still_answers() {
    let dir = tempfile::tempdir().expect("temp dir");
    seed_archive(dir.path());
    let app = app(None, Some(dir.path()), Some(Duration::from_secs(30)));

    let reply = call(&app, "/api/calendar?country=Testland&from=2007&to=2010").await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);

    let v = reply.json();
    let failures = v["fetch"]["failures"].as_array().expect("failures array");
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["year"], 2007);
    assert_eq!(failures[0]["sensor"], "ModisTerra");
    let error = failures[0]["error"].as_str().expect("error string");
    assert!(error.contains("not in the local archive"), "error: {error}");

    assert_eq!(v["calendar"].as_array().map(|items| items.len()), Some(4));
    assert_eq!(v["effort"]["reliable"].as_bool(), Some(false));
    let warnings = v["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|warning| warning
            .as_str()
            .unwrap_or_default()
            .contains("1 FIRMS archive download failed")),
        "warnings: {warnings:?}"
    );
}

#[tokio::test]
async fn calendar_bbox_keeps_only_points_inside() {
    let dir = tempfile::tempdir().expect("temp dir");
    seed_archive(dir.path());
    let app = app(None, Some(dir.path()), Some(Duration::from_secs(30)));

    let reply = call(
        &app,
        "/api/calendar?country=Testland&from=2008&to=2010&west=29.5&south=36.95&east=30.5&north=37.05",
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);

    let v = reply.json();
    let cells = v["calendar"].as_array().expect("calendar array");
    assert_eq!(cells.len(), 3);
    let keys: Vec<(i64, i64)> = cells
        .iter()
        .map(|cell| {
            (
                cell["year"].as_i64().expect("year"),
                cell["month"].as_i64().expect("month"),
            )
        })
        .collect();
    assert_eq!(keys, vec![(2008, 7), (2009, 7), (2010, 7)]);
    for cell in cells {
        assert_eq!(cell["count"], 1);
        assert!(
            cell["anomaly"].is_null(),
            "identical counts have zero spread: {:?}",
            cell["anomaly"]
        );
    }

    let south = v["bbox"]["south"].as_f64().expect("bbox south");
    assert!((south - 36.95).abs() < 1e-9, "bbox south {south}");
}

#[tokio::test]
async fn calendar_without_archive_directory_returns_500() {
    let app = app(None, None, None);
    let reply = call(&app, "/api/calendar?country=Testland&from=2008&to=2010").await;
    assert_eq!(
        reply.status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "body: {}",
        reply.body
    );
    assert_eq!(
        error_text(&reply),
        "FIRMS archive cache directory is unavailable"
    );
}
