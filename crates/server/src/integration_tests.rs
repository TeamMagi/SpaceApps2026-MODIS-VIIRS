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

// Tests that only need "some valid day" use this fixed one, so their result never depends on
// the wall clock.
fn fixed_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, 15).expect("valid date")
}

// The server resolves `days=N` against the real UTC clock (FirmsClient::fetch_all), so tests
// that go through that path cannot use a fixed date and must mock the real "today".
fn today() -> NaiveDate {
    Utc::now().date_naive()
}

// Waits out the last seconds before UTC midnight so a test never straddles a date change.
async fn wait_clear_of_utc_midnight() {
    let now = Utc::now();
    let next_midnight = (now.date_naive() + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc();
    let remaining = next_midnight - now;
    if remaining < chrono::Duration::seconds(15) {
        tokio::time::sleep(remaining.to_std().unwrap_or_default() + Duration::from_millis(50))
            .await;
    }
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
    wait_clear_of_utc_midnight().await;
    healthy_firms_on(today()).await
}

async fn healthy_firms_on(date: NaiveDate) -> MockServer {
    let server = MockServer::start().await;
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
    app_with_config(
        firms,
        archive_dir,
        request_timeout,
        pyrosync_core::config::PipelineConfig::default(),
    )
}

fn app_with_config(
    firms: Option<FirmsClient>,
    archive_dir: Option<&Path>,
    request_timeout: Option<Duration>,
    config: pyrosync_core::config::PipelineConfig,
) -> Router {
    let state = AppState {
        firms: firms.map(Arc::new),
        archive: archive_dir.map(|dir| Arc::new(ArchiveClient::offline(dir.to_path_buf()))),
        cache: Arc::new(RwLock::new(ResponseCache::new(8 * 1024 * 1024))),
        request_timeout,
        config: Arc::new(config),
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
    let warnings = v["warnings"].as_array().expect("warnings array");
    assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
    assert!(
        warnings[0]
            .as_str()
            .unwrap_or_default()
            .contains("MODIS orbital drift"),
        "warnings: {warnings:?}"
    );
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
    let firms = healthy_firms_on(fixed_day()).await;
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

    let min = ymd(fixed_day() - chrono::Duration::days(60));
    let max = ymd(fixed_day() + chrono::Duration::days(2));
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
    assert_eq!(warnings.len(), 2, "warnings: {warnings:?}");
    assert!(
        warnings[0]
            .as_str()
            .unwrap_or_default()
            .contains("1 FIRMS request failed"),
        "warnings: {warnings:?}"
    );
    assert!(
        warnings[1]
            .as_str()
            .unwrap_or_default()
            .contains("MODIS orbital drift"),
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
                .set_delay(Duration::from_secs(2)),
        )
        .with_priority(1)
        .mount(&firms)
        .await;

    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_millis(100)),
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
async fn firms_endpoints_without_api_key_return_503_with_reason(#[case] uri: &str) {
    let app = app(None, None, None);
    let reply = call(&app, uri).await;
    assert_eq!(
        reply.status,
        StatusCode::SERVICE_UNAVAILABLE,
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
    // Only the days-based and default-window cases depend on the real clock.
    let explicit_range = query.contains("{today}");
    let day = if explicit_range { fixed_day() } else { today() };
    let firms = if explicit_range {
        healthy_firms_on(day).await
    } else {
        healthy_firms().await
    };
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let query = query
        .replace("{today}", &ymd(day))
        .replace("{yesterday}", &ymd(day - chrono::Duration::days(1)));
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

const SP_DATE: &str = "2024-07-01";
const SP_SOURCES: [&str; 4] = [
    "MODIS_SP",
    "VIIRS_SNPP_SP",
    "VIIRS_NOAA20_SP",
    "VIIRS_NOAA21_SP",
];

fn sp_availability_csv() -> &'static str {
    "data_id,min_date,max_date\n\
     MODIS_SP,2024-01-01,2024-12-31\n\
     VIIRS_SNPP_SP,2024-01-01,2024-12-31\n\
     VIIRS_NOAA20_SP,2024-01-01,2024-12-31\n\
     VIIRS_NOAA21_SP,2024-01-01,2024-12-31\n"
}

async fn healthy_sp_firms() -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!("/api/data_availability/csv/{KEY}/ALL")))
        .respond_with(ResponseTemplate::new(200).set_body_string(sp_availability_csv()))
        .mount(&server)
        .await;

    mount_area(
        &server,
        "MODIS_SP",
        ResponseTemplate::new(200).set_body_string(modis_csv(SP_DATE)),
    )
    .await;
    mount_area(
        &server,
        "VIIRS_SNPP_SP",
        ResponseTemplate::new(200).set_body_string(viirs_snpp_csv(SP_DATE)),
    )
    .await;
    mount_area(
        &server,
        "VIIRS_NOAA20_SP",
        ResponseTemplate::new(200).set_body_string(format!("{VIIRS_HEADER}\n")),
    )
    .await;
    mount_area(
        &server,
        "VIIRS_NOAA21_SP",
        ResponseTemplate::new(200).set_body_string(format!("{VIIRS_HEADER}\n")),
    )
    .await;

    server
}

fn cached_firms_client(api_root: impl Into<String>, cache_dir: &Path) -> FirmsClient {
    FirmsClient::new(KEY.to_string())
        .expect("client builds")
        .with_api_root(api_root)
        .with_area_cache(AreaCache::new(
            cache_dir.to_path_buf(),
            Duration::from_secs(600),
        ))
}

#[tokio::test]
async fn area_cache_makes_second_identical_range_free() {
    let firms = healthy_sp_firms().await;
    let cache_dir = tempfile::tempdir().expect("temp dir");
    let uri = format!("/api/harmonize?{BBOX}&start={SP_DATE}&end={SP_DATE}");

    let app1 = app(
        Some(cached_firms_client(firms.uri(), cache_dir.path())),
        None,
        Some(Duration::from_secs(30)),
    );
    let first = call(&app1, &uri).await;
    assert_eq!(first.status, StatusCode::OK, "body: {}", first.body);
    assert_eq!(first.x_cache.as_deref(), Some("MISS"));

    let first_json = first.json();
    let first_count = first_json["harmonized"]["count"]
        .as_u64()
        .expect("harmonized count");
    assert_eq!(first_count, 3);
    let first_spent = first_json["report"]["transactions_spent"]
        .as_u64()
        .expect("transactions_spent");
    assert!(first_spent > 0, "expected transactions > 0, got {first_spent}");
    for source in SP_SOURCES {
        assert_eq!(area_requests(&firms, source).await, 1, "source {source}");
    }

    // Separate router instance so the in-memory ResponseCache cannot satisfy the second call.
    let app2 = app(
        Some(cached_firms_client(firms.uri(), cache_dir.path())),
        None,
        Some(Duration::from_secs(30)),
    );
    let second = call(&app2, &uri).await;
    assert_eq!(second.status, StatusCode::OK, "body: {}", second.body);
    assert_eq!(second.x_cache.as_deref(), Some("MISS"));

    let second_json = second.json();
    assert_eq!(second_json["report"]["transactions_spent"], 0);
    assert_eq!(second_json["harmonized"]["count"], first_count);
    for source in SP_SOURCES {
        assert_eq!(
            area_requests(&firms, source).await,
            1,
            "second call should not hit area endpoint for {source}"
        );
    }
}

#[tokio::test]
async fn area_cache_serves_cached_range_when_firms_is_unreachable() {
    let firms = healthy_sp_firms().await;
    let live_uri = firms.uri();
    let cache_dir = tempfile::tempdir().expect("temp dir");
    let uri = format!("/api/harmonize?{BBOX}&start={SP_DATE}&end={SP_DATE}");

    let app1 = app(
        Some(cached_firms_client(&live_uri, cache_dir.path())),
        None,
        Some(Duration::from_secs(30)),
    );
    let first = call(&app1, &uri).await;
    assert_eq!(first.status, StatusCode::OK, "body: {}", first.body);
    let first_json = first.json();
    let first_count = first_json["harmonized"]["count"]
        .as_u64()
        .expect("harmonized count");
    assert_eq!(first_count, 3);
    assert!(
        first_json["report"]["transactions_spent"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );

    drop(firms);

    // wiremock pools its servers, so the dropped mock's port can be handed to another test
    // running in parallel; point the second client at a port that nothing listens on instead.
    let closed_port = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port();
    let dead_uri = format!("http://127.0.0.1:{closed_port}");

    let app2 = app(
        Some(cached_firms_client(&dead_uri, cache_dir.path())),
        None,
        Some(Duration::from_secs(30)),
    );
    let second = call(&app2, &uri).await;
    assert_eq!(second.status, StatusCode::OK, "body: {}", second.body);
    assert_eq!(second.x_cache.as_deref(), Some("MISS"));

    let second_json = second.json();
    assert_eq!(second_json["harmonized"]["count"], first_count);
    assert_eq!(second_json["report"]["transactions_spent"], 0);
}

#[tokio::test]
async fn area_cache_does_not_store_failed_windows() {
    let firms = healthy_sp_firms().await;
    // Four SP sources each issue one unretried area request before falling through to the 200 mocks.
    Mock::given(method("GET"))
        .and(path_regex(format!("^/api/area/csv/{KEY}/")))
        .respond_with(
            ResponseTemplate::new(400).set_body_string("Exceeding allowed transaction limit"),
        )
        .up_to_n_times(4)
        .with_priority(1)
        .mount(&firms)
        .await;

    let cache_dir = tempfile::tempdir().expect("temp dir");
    let uri = format!("/api/harmonize?{BBOX}&start={SP_DATE}&end={SP_DATE}");

    let app1 = app(
        Some(cached_firms_client(firms.uri(), cache_dir.path())),
        None,
        Some(Duration::from_secs(30)),
    );
    let first = call(&app1, &uri).await;
    assert_eq!(first.status, StatusCode::BAD_GATEWAY, "body: {}", first.body);
    assert!(
        error_text(&first).contains("Exceeding allowed transaction limit"),
        "error: {}",
        first.body
    );

    let app2 = app(
        Some(cached_firms_client(firms.uri(), cache_dir.path())),
        None,
        Some(Duration::from_secs(30)),
    );
    let second = call(&app2, &uri).await;
    assert_eq!(second.status, StatusCode::OK, "body: {}", second.body);

    let second_json = second.json();
    assert_eq!(second_json["harmonized"]["count"], 3);
    assert_eq!(second_json["report"]["failures"], json!([]));
    let second_spent = second_json["report"]["transactions_spent"]
        .as_u64()
        .expect("transactions_spent");
    assert!(
        second_spent > 0,
        "expected transactions > 0, got {second_spent}"
    );
    for source in SP_SOURCES {
        assert_eq!(area_requests(&firms, source).await, 2, "source {source}");
    }
}

// ---------- C12: min_confidence filter and atmospheric bias note ----------

#[tokio::test]
async fn default_min_confidence_keeps_every_detection_and_echoes_low() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let implicit = call(&app, &format!("/api/harmonize?{BBOX}&days=1")).await;
    assert_eq!(implicit.status, StatusCode::OK, "body: {}", implicit.body);
    let v = implicit.json();
    assert_eq!(v["raw"]["total_raw"], 5);
    assert_eq!(v["harmonized"]["count"], 3);
    assert_eq!(
        v["filters"],
        json!({"min_confidence": "low", "excluded_detections": 0})
    );

    // `low` is the default, so the explicit form shares the cache entry with the implicit one.
    let explicit = call(&app, &format!("/api/harmonize?{BBOX}&days=1&min_confidence=low")).await;
    assert_eq!(explicit.status, StatusCode::OK, "body: {}", explicit.body);
    assert_eq!(explicit.x_cache.as_deref(), Some("HIT"));
    assert_eq!(explicit.body, implicit.body);
}

#[tokio::test]
async fn nominal_min_confidence_drops_low_detections_before_harmonizing() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let unfiltered = call(&app, &format!("/api/harmonize?{BBOX}&days=1")).await;
    assert_eq!(unfiltered.status, StatusCode::OK, "body: {}", unfiltered.body);

    // VIIRS row 3 is the only `l` detection (and the only ViirsOnly record).
    let reply = call(
        &app,
        &format!("/api/harmonize?{BBOX}&days=1&include_raw=true&min_confidence=nominal"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);
    assert_eq!(
        reply.x_cache.as_deref(),
        Some("MISS"),
        "a filtered request must not reuse the unfiltered cache entry"
    );
    let v = reply.json();
    assert_eq!(v["raw"]["modis_count"], 2);
    assert_eq!(v["raw"]["viirs_count"], 2);
    assert_eq!(v["raw"]["total_raw"], 4);
    assert_eq!(v["raw"]["hotspots"].as_array().map(Vec::len), Some(4));
    assert_eq!(v["harmonized"]["count"], 2);
    let mut sources: Vec<&str> = v["harmonized"]["hotspots"]
        .as_array()
        .expect("hotspots array")
        .iter()
        .map(|h| h["source"].as_str().expect("source"))
        .collect();
    sources.sort_unstable();
    assert_eq!(sources, vec!["Matched", "ModisOnly"]);
    assert_eq!(
        v["filters"],
        json!({"min_confidence": "nominal", "excluded_detections": 1})
    );

    let compare = call(&app, &format!("/api/compare?{BBOX}&days=1&min_confidence=nominal")).await;
    assert_eq!(compare.status, StatusCode::OK, "body: {}", compare.body);
    let c = compare.json();
    assert_eq!(c["counts"]["raw_total"], 4);
    assert_eq!(c["counts"]["raw_viirs"], 2);
    assert_eq!(c["counts"]["harmonized_total"], 2);
    assert_eq!(
        c["counts"]["by_source"],
        json!({"modis_only": 1, "viirs_only": 0, "matched": 1})
    );
    assert_eq!(c["filters"]["min_confidence"], "nominal");
    assert_eq!(c["filters"]["excluded_detections"], 1);

    let stats = call(&app, &format!("/api/stats?{BBOX}&days=1&min_confidence=nominal")).await;
    assert_eq!(stats.status, StatusCode::OK, "body: {}", stats.body);
    let s = stats.json();
    assert_eq!(s["totals"]["raw_detections"], 4);
    assert_eq!(s["totals"]["harmonized_records"], 2);
    assert_eq!(s["confidence_breakdown"]["low"], 0);
    assert_eq!(s["filters"]["excluded_detections"], 1);

    let export = call(&app, &format!("/api/export?{BBOX}&days=1&min_confidence=nominal")).await;
    assert_eq!(export.status, StatusCode::OK, "body: {}", export.body);
    let rows = export.body.lines().filter(|line| !line.is_empty()).count();
    assert_eq!(rows, 1 + 2, "header plus two harmonized rows: {}", export.body);
    let unfiltered_export = call(&app, &format!("/api/export?{BBOX}&days=1")).await;
    assert_eq!(unfiltered_export.status, StatusCode::OK);
    assert_eq!(unfiltered_export.x_cache.as_deref(), Some("MISS"));
    let unfiltered_rows = unfiltered_export.body.lines().filter(|line| !line.is_empty()).count();
    assert_eq!(unfiltered_rows, 1 + 3);
}

#[tokio::test]
async fn high_min_confidence_keeps_only_high_detections() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    // MODIS 80 and VIIRS `h` are High; MODIS 60, VIIRS `n` and `l` are dropped.
    let reply = call(&app, &format!("/api/compare?{BBOX}&days=1&min_confidence=high")).await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);
    let v = reply.json();
    assert_eq!(v["counts"]["raw_modis"], 1);
    assert_eq!(v["counts"]["raw_viirs"], 1);
    assert_eq!(v["counts"]["harmonized_total"], 1);
    assert_eq!(
        v["filters"],
        json!({"min_confidence": "high", "excluded_detections": 3})
    );
}

#[rstest]
#[case::harmonize("/api/harmonize")]
#[case::compare("/api/compare")]
#[case::stats("/api/stats")]
#[case::export("/api/export")]
#[tokio::test]
async fn invalid_min_confidence_is_rejected_before_reaching_firms(#[case] route: &str) {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let reply = call(&app, &format!("{route}?{BBOX}&days=1&min_confidence=medium")).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "body: {}", reply.body);
    assert_eq!(
        error_text(&reply),
        "Invalid 'min_confidence' value 'medium': expected 'low', 'nominal' or 'high'"
    );
    let paths = upstream_paths(&firms).await;
    assert!(paths.is_empty(), "{route} should not reach FIRMS, got {paths:?}");
}

#[tokio::test]
async fn harmonize_uncertainty_carries_one_sided_atmospheric_bias_note() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let reply = call(&app, &format!("/api/harmonize?{BBOX}&days=1")).await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);
    let bias = &reply.json()["uncertainty"]["atmospheric_bias"];
    assert_eq!(bias["direction"], "downward");
    assert_eq!(bias["low_pct"], 7);
    assert_eq!(bias["high_pct"], 22);
    assert_eq!(bias["applied"], false);
    let note = bias["note"].as_str().expect("note string");
    assert!(note.contains("tau_4") && note.contains("7-22%"), "note: {note}");
    let refs = bias["refs"].as_array().expect("refs array");
    assert_eq!(refs.len(), 3);
    assert!(refs.iter().all(|r| r.as_str().is_some_and(|r| r.starts_with('['))));
}

#[tokio::test]
async fn calendar_min_confidence_filters_archive_rows_and_keeps_bias_note() {
    let dir = tempfile::tempdir().expect("temp dir");
    seed_archive(dir.path());
    // MODIS confidence 20 parses as Low.
    let low_rows: Vec<String> = (1..=3u32)
        .map(|day| {
            archive_row(&format!("2011-07-{day:02}"), 37.0 + f64::from(day) * 0.1, 0)
                .replace(",MODIS,80,", ",MODIS,20,")
        })
        .collect();
    write_archive_year(dir.path(), 2011, &low_rows);
    let app = app(None, Some(dir.path()), Some(Duration::from_secs(30)));

    let all = call(&app, "/api/calendar?country=Testland&from=2010&to=2011").await;
    assert_eq!(all.status, StatusCode::OK, "body: {}", all.body);
    let v = all.json();
    assert_eq!(v["counts"]["raw_detections"], 7 + 3);
    assert_eq!(v["filters"]["min_confidence"], "low");
    assert_eq!(v["uncertainty"]["atmospheric_bias"]["direction"], "downward");

    let nominal = call(
        &app,
        "/api/calendar?country=Testland&from=2010&to=2011&min_confidence=nominal",
    )
    .await;
    assert_eq!(nominal.status, StatusCode::OK, "body: {}", nominal.body);
    assert_eq!(nominal.x_cache.as_deref(), Some("MISS"));
    let n = nominal.json();
    assert_eq!(n["counts"]["raw_detections"], 7);
    assert_eq!(
        n["filters"],
        json!({"min_confidence": "nominal", "excluded_detections": 3})
    );
    let years: Vec<i64> = n["calendar"]
        .as_array()
        .expect("calendar array")
        .iter()
        .map(|cell| cell["year"].as_i64().expect("year"))
        .collect();
    assert!(!years.contains(&2011), "2011 held only Low rows: {years:?}");

    let bad = call(&app, "/api/calendar?country=Testland&min_confidence=x").await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "body: {}", bad.body);
}

#[tokio::test]
async fn calendar_static_mask_uses_low_confidence_detections_even_when_filtered() {
    let dir = tempfile::tempdir().expect("temp dir");
    // Cell at 37.0°N, 30.0°E burns across 11 months in 2010: 10 Low months + 1 High month.
    // A second cell at 38.0°N, 30.0°E burns once in November (High) and is not persistent.
    let mut rows: Vec<String> = (1..=10u32)
        .map(|month| {
            archive_row(&format!("2010-{month:02}-01"), 37.0, 0)
                .replace(",MODIS,80,", ",MODIS,20,")
        })
        .collect();
    rows.push(archive_row("2010-11-01", 37.0, 0));
    rows.push(archive_row("2010-11-02", 38.0, 0));
    write_archive_year(dir.path(), 2010, &rows);

    let app = app(None, Some(dir.path()), Some(Duration::from_secs(30)));

    let nominal = call(
        &app,
        "/api/calendar?country=Testland&from=2010&to=2010&min_confidence=nominal",
    )
    .await;
    assert_eq!(nominal.status, StatusCode::OK, "body: {}", nominal.body);
    let n = nominal.json();
    assert_eq!(n["counts"]["raw_detections"], 2);
    assert_eq!(n["counts"]["harmonized_records"], 1);
    assert_eq!(
        n["filters"],
        json!({"min_confidence": "nominal", "excluded_detections": 10})
    );
    let cells = n["calendar"].as_array().expect("calendar array");
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0]["year"], 2010);
    assert_eq!(cells[0]["month"], 11);
    assert_eq!(cells[0]["count"], 1);
}

// ---------- C10: tropical swath-gap flag ----------

/// Testland rows in July 2010 at 5°N (inside the belt) and 35°N (outside), one per day.
fn seed_tropical_archive(root: &Path) {
    write_archive_year(
        root,
        2010,
        &[
            archive_row("2010-07-01", 5.0, 0),
            archive_row("2010-07-02", 35.0, 0),
        ],
    );
}

#[tokio::test]
async fn calendar_effort_carries_tropical_swath_gap_flag_only_for_belt_regions() {
    let dir = tempfile::tempdir().expect("temp dir");
    seed_archive(dir.path());
    let mid_lat_app = app(None, Some(dir.path()), Some(Duration::from_secs(30)));
    let mid_lat = call(&mid_lat_app, "/api/calendar?country=Testland&from=2008&to=2010").await;
    assert_eq!(mid_lat.status, StatusCode::OK, "body: {}", mid_lat.body);
    let v = mid_lat.json();
    assert!(v["effort"].as_object().unwrap().contains_key("tropical_swath_gap"));
    assert!(v["effort"]["tropical_swath_gap"].is_null());

    let tropical_dir = tempfile::tempdir().expect("temp dir");
    seed_tropical_archive(tropical_dir.path());
    let app = app(None, Some(tropical_dir.path()), Some(Duration::from_secs(30)));

    let extent = call(&app, "/api/calendar?country=Testland&from=2010&to=2010").await;
    assert_eq!(extent.status, StatusCode::OK, "body: {}", extent.body);
    let flag = &extent.json()["effort"]["tropical_swath_gap"];
    assert_eq!(flag["basis"], "detections");
    assert_eq!(flag["belt_abs_lat"], 30.0);
    assert_eq!(flag["swath_gap_km"], 388.1);
    assert_eq!(flag["missed_frp_low_pct"], 10.7);
    assert_eq!(flag["missed_frp_high_pct"], 12.5);
    assert!(flag["compensation"].is_null());
    let share = flag["belt_share"].as_f64().expect("belt share");
    let expected = pyrosync_core::effort::tropical_belt_share(5.0, 35.0);
    assert!((share - expected).abs() < 1e-12, "{share} vs {expected}");
    assert!(flag["reference"].as_str().unwrap().starts_with("[14]"));

    let bbox = call(
        &app,
        "/api/calendar?country=Testland&from=2010&to=2010&west=29&south=-5&east=31&north=10",
    )
    .await;
    assert_eq!(bbox.status, StatusCode::OK, "body: {}", bbox.body);
    let flag = &bbox.json()["effort"]["tropical_swath_gap"];
    assert_eq!(flag["basis"], "bbox");
    assert_eq!(flag["belt_share"], 1.0);
}

#[tokio::test]
async fn calendar_swath_gap_compensation_scales_tropical_modis_total_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    seed_tropical_archive(dir.path());
    let uri = "/api/calendar?country=Testland&from=2010&to=2010";

    let off = app(None, Some(dir.path()), Some(Duration::from_secs(30)));
    let off = call(&off, uri).await.json();

    let mut config = pyrosync_core::config::PipelineConfig::default();
    config.uncertainty.tropical_swath_gap_compensation = true;
    let on = app_with_config(None, Some(dir.path()), Some(Duration::from_secs(30)), config);
    let on = call(&on, uri).await.json();

    let comp = &on["effort"]["tropical_swath_gap"]["compensation"];
    let factor = comp["factor"].as_f64().expect("factor");
    assert!(comp["factor_sigma"].as_f64().expect("sigma") > 0.0);

    // One 25 MW Terra row at 5°N (k_c 1.11, compensated) and one at 35°N (k_c 1.18, not).
    let total = |v: &Value| v["calendar"][0]["total_frp"].as_f64().expect("total");
    let width = |v: &Value| {
        let c = &v["calendar"][0];
        c["total_frp_upper"].as_f64().unwrap() - c["total_frp_lower"].as_f64().unwrap()
    };
    // Both rows share the same scan, so they carry the same VZA-corrected FRP.
    let per_row = total(&off) / (1.11 + 1.18);
    let expected_on = per_row * (1.11 * factor + 1.18);
    assert!((total(&on) - expected_on).abs() < 1e-6, "{} vs {expected_on}", total(&on));
    assert!(total(&on) > total(&off));
    assert!(width(&on) > width(&off));
    assert_eq!(on["calendar"][0]["count"], off["calendar"][0]["count"]);
    assert_eq!(
        on["calendar"][0]["count_normalized"],
        off["calendar"][0]["count_normalized"]
    );
}

#[tokio::test]
async fn harmonize_effort_reports_no_swath_gap_outside_the_belt() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );
    let reply = call(&app, &format!("/api/harmonize?{BBOX}&days=1")).await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);
    let effort = reply.json()["effort"].clone();
    assert!(effort.as_object().unwrap().contains_key("tropical_swath_gap"));
    assert!(effort["tropical_swath_gap"].is_null());
}

#[tokio::test]
async fn nrt_static_mask_uses_low_confidence_detections_and_reports_counts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/data_availability/csv/{KEY}/ALL")))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "data_id,min_date,max_date\nMODIS_NRT,2025-01-01,2025-11-05\n",
        ))
        .mount(&server)
        .await;

    let mut csv = format!("{MODIS_HEADER}\n");
    for month in 1..=10u32 {
        csv.push_str(&format!(
            "37.0000,30.0000,330.5,1.0,1.0,2025-{month:02}-01,1000,Terra,MODIS,20,6.1NRT,295.0,50.0,D\n"
        ));
    }
    csv.push_str(
        "37.0000,30.0000,330.5,1.0,1.0,2025-11-01,1000,Terra,MODIS,80,6.1NRT,295.0,50.0,D\n",
    );
    csv.push_str(
        "38.0000,30.0000,330.5,1.0,1.0,2025-11-02,1000,Terra,MODIS,80,6.1NRT,295.0,50.0,D\n",
    );

    mount_area(
        &server,
        "MODIS_NRT",
        ResponseTemplate::new(200).set_body_string(format!("{MODIS_HEADER}\n")),
    )
    .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/area/csv/{KEY}/MODIS_NRT/26,36,45,42/5/2025-01-01"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(csv))
        .with_priority(1)
        .mount(&server)
        .await;

    let app = app(
        Some(firms_client(&server)),
        None,
        Some(Duration::from_secs(30)),
    );
    let query = format!("{BBOX}&start=2025-01-01&end=2025-11-02&min_confidence=nominal");

    let harmonize = call(&app, &format!("/api/harmonize?{query}")).await;
    assert_eq!(harmonize.status, StatusCode::OK, "body: {}", harmonize.body);
    let h = harmonize.json();
    assert_eq!(h["raw"]["total_raw"], 2);
    assert_eq!(h["harmonized"]["count"], 1);
    assert_eq!(h["harmonized"]["static_masked"], 1);
    assert_eq!(h["harmonized"]["sama_removed"], 0);
    assert_eq!(
        h["filters"],
        json!({"min_confidence": "nominal", "excluded_detections": 10})
    );

    let compare = call(&app, &format!("/api/compare?{query}")).await;
    assert_eq!(compare.status, StatusCode::OK, "body: {}", compare.body);
    let c = compare.json();
    assert_eq!(c["counts"]["harmonized_total"], 1);
    assert_eq!(c["counts"]["static_masked"], 1);
    assert_eq!(c["counts"]["sama_removed"], 0);

    let stats = call(&app, &format!("/api/stats?{query}")).await;
    assert_eq!(stats.status, StatusCode::OK, "body: {}", stats.body);
    let s = stats.json();
    assert_eq!(s["totals"]["harmonized_records"], 1);
    assert_eq!(s["totals"]["static_masked"], 1);
    assert_eq!(s["totals"]["sama_removed"], 0);
}

fn sensitivity_cell(v: &Value, radius_km: f64, window_min: u64) -> Value {
    v["cells"]
        .as_array()
        .expect("cells array")
        .iter()
        .find(|c| c["radius_km"].as_f64() == Some(radius_km) && c["window_min"] == window_min)
        .unwrap_or_else(|| panic!("cell {radius_km}/{window_min} missing in {v}"))
        .clone()
}

#[tokio::test]
async fn sensitivity_matched_never_decreases_with_radius_or_window() {
    let firms = healthy_firms().await;
    let app = app(
        Some(firms_client(&firms)),
        None,
        Some(Duration::from_secs(30)),
    );

    let uri = format!("/api/sensitivity?{BBOX}&days=1&radius_km=0.05,0.5,2,10&window_min=30,60,180");
    let reply = call(&app, &uri).await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);
    let v = reply.json();
    assert_eq!(v["raw_modis"], 2);
    assert_eq!(v["raw_viirs"], 3);
    assert_eq!(v["cells"].as_array().expect("cells").len(), 12);

    let radii = [0.05, 0.5, 2.0, 10.0];
    let windows = [30_u64, 60, 180];
    let matched = |r: f64, w: u64| sensitivity_cell(&v, r, w)["matched"].as_u64().expect("matched");
    for (ri, &r) in radii.iter().enumerate() {
        for (wi, &w) in windows.iter().enumerate() {
            if ri > 0 {
                assert!(matched(r, w) >= matched(radii[ri - 1], w), "radius {r} window {w}");
            }
            if wi > 0 {
                assert!(matched(r, w) >= matched(r, windows[wi - 1]), "radius {r} window {w}");
            }
        }
    }
    // The only pair is 0.14 km and 60 min apart.
    assert_eq!(matched(0.05, 180), 0);
    assert_eq!(matched(0.5, 30), 0);
    assert_eq!(matched(0.5, 60), 1);
    assert_eq!(matched(2.0, 180), 1);
    let cell = sensitivity_cell(&v, 2.0, 180);
    assert_eq!(cell["modis_only"], 1);
    assert_eq!(cell["viirs_only"], 1);
    assert_eq!(cell["harmonized_total"], 3);
}

#[rstest]
#[case::radius_zero("0")]
#[case::radius_negative("-1")]
#[case::radius_above_max("10.001")]
#[case::radius_nan("NaN")]
#[case::radius_text("abc")]
#[case::radius_one_bad_in_list("1,10.5")]
#[tokio::test]
async fn sensitivity_rejects_bad_radius(#[case] radius: &str) {
    let app = app(None, None, None);
    let reply = call(
        &app,
        &format!("/api/sensitivity?{BBOX}&days=1&radius_km={radius}&window_min=60"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "body: {}", reply.body);
    assert!(reply.json()["error"].is_string());
}

#[rstest]
#[case::radius_just_above_zero("0.001", "60")]
#[case::radius_at_max("10", "60")]
#[case::window_zero("2", "0")]
#[case::window_at_max("2", "1440")]
#[tokio::test]
async fn sensitivity_accepts_boundary_values(#[case] radius: &str, #[case] window: &str) {
    let firms = healthy_firms().await;
    let app = app(Some(firms_client(&firms)), None, Some(Duration::from_secs(30)));
    let reply = call(
        &app,
        &format!("/api/sensitivity?{BBOX}&days=1&radius_km={radius}&window_min={window}"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "body: {}", reply.body);
}

#[rstest]
#[case::radius_six_values("1,2,3,4,5,6", "60")]
#[case::window_six_values("2", "10,20,30,40,50,60")]
#[case::window_above_max("2", "1441")]
#[case::window_negative("2", "-5")]
#[tokio::test]
async fn sensitivity_rejects_oversized_grid_or_bad_window(
    #[case] radius: &str,
    #[case] window: &str,
) {
    let app = app(None, None, None);
    let reply = call(
        &app,
        &format!("/api/sensitivity?{BBOX}&days=1&radius_km={radius}&window_min={window}"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "body: {}", reply.body);
    assert!(reply.json()["error"].is_string());
}

#[rstest]
#[case::no_window("/api/sensitivity?west=26&south=36&east=45&north=42&days=1&radius_km=2")]
#[case::no_radius("/api/sensitivity?west=26&south=36&east=45&north=42&days=1&window_min=60")]
#[case::no_bbox_part("/api/sensitivity?west=26&south=36&east=45&days=1&radius_km=2&window_min=60")]
#[tokio::test]
async fn sensitivity_rejects_missing_parameter(#[case] uri: &str) {
    let app = app(None, None, None);
    let reply = call(&app, uri).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "body: {}", reply.body);
    assert!(reply.json()["error"].is_string());
}
