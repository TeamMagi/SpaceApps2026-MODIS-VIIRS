# PyroSync İzlenebilirlik Matrisi

| Alan | Değer |
|------|-------|
| Belge kimliği | PYRO-RTM-001 |
| Durum tarihi | 2026-11-15, birleşmiş `main` |
| Üst belgeler | [TEST_PLANI.md](TEST_PLANI.md), [TEST_TASARIMI.md](TEST_TASARIMI.md) |
| Gereksinim kaynakları | `GOREVLER.md` ("Bitti sayılır" şartları ve görev notları), `docs/API.md`, `crates/*/CLAUDE.md` |

Son koşu (2026-11-15, birleşmiş `main`): Rust 600 geçti, 0 başarısız, 2 yoksayıldı (core 273, firms-client 140 + 9, server 178); web 32 geçti; clippy temiz; satır kapsamı %94,7. S6 (34 test, `78785f2`), S9 (varsayılan σ sabitleme), S11 (`/api/sensitivity`, `viirs_zone`) ve firms-client testleri (`6400ff4`, 70 test) birleşmiştir ve aşağıda yer alır. CI (`ci.yml`) tanımlıdır; GitHub'da yeşil çalıştığı **doğrulanmadı** (depo özel).

**Durum anlamı.** Kapsandı: bağlı vakaların hepsinin koşulunu sınayan otomatik test var. Kısmi: bazı vakalar `EKSİK` veya yalnızca temsilci değer denendi. EKSİK: otomatik test yok. "Kapsandı", gereksinimin doğru olduğunu değil, davranışın sabitlendiğini söyler; bilimsel doğruluk için bkz. REQ-SCI satırları.

Otomatik test sütunu kısaltmaları TEST_TASARIMI §0 ile aynıdır; test işlev adları tam olarak oradaki tablolarda geçer, burada yalnız öne çıkanlar verilir.

## 1. HTTP API

| REQ | Gereksinim | Kaynak | Vaka | Otomatik test (öne çıkan) | Durum |
|-----|------------|--------|------|---------------------------|-------|
| REQ-API-001 | Bbox: koordinat aralıkları ve sıra doğrulanır, geçersizse 400 ve FIRMS'e istek gitmez | `docs/API.md` Bounding box; S2 | TC-API-001..017 | `srv::it::harmonize_query_validation` (13 bbox vakası) | Kısmi (014, 015, 016 EKSİK; `compare/stats/export` için yalnız tek vaka) |
| REQ-API-002 | Zaman penceresi: `days` 1..=5 ya da `start`+`end`, ikisi birlikte olmaz | `docs/API.md` Zaman penceresi; S2 | TC-API-020..052 | `srv::it::harmonize_query_validation` (16 vaka) | Kısmi (022, 023, 030, 048, 049 EKSİK; 050 belirtim boşluğu) |
| REQ-API-003 | Hata gövdesi her zaman `{"error": "..."}` | `docs/API.md` Genel bilgiler; BUG-2 | TC-API-013, TC-DT-020 | `srv::main::query_deserialization_errors_return_json_400` | Kapsandı |
| REQ-API-004 | Doğrulama FIRMS anahtarı denetiminden önce yapılır | `docs/API.md` harmonize Hatalar | TC-DT-021, 022, 026 | `srv::main::handlers_enforce_validation_before_firms_client` | Kısmi (birleşik durum 026 EKSİK) |
| REQ-API-005 | Anahtar yoksa FIRMS uç noktaları 503, arşiv uç noktaları çalışır | `docs/API.md` Genel bilgiler | TC-DT-022 | `srv::it::firms_endpoints_without_api_key_return_503_with_reason`, `srv::main::firms_endpoints_return_503_when_api_key_missing` | Kapsandı |
| REQ-API-006 | Tüm kaynaklar başarısız → 502; süre aşımı → 504 | `docs/API.md` Hata kodları; G4 | TC-DT-023, 024, 027 | `srv::it::rejected_key_body_returns_502_without_leaking_the_key`, `slow_upstream_hits_request_timeout_with_504` | Kısmi (TC-DT-027: belge "tüm istekler" der, kod "veri yok ve ≥1 hata" der; hangisi doğru karar bekliyor) |
| REQ-API-007 | Kısmi FIRMS hatasında 200, `warnings` dolu, `effort.reliable=false` | G4 ("G3'ten devralınan risk kapatıldı"); `docs/API.md` effort | TC-DT-025, TC-API-115 | `srv::it::one_failing_source_degrades_to_200_with_warning`, `calendar_reports_missing_archive_year_but_still_answers` | Kapsandı |
| REQ-API-008 | Yanıt önbelleği: `X-Cache`, TTL (600 s / 21600 s), hata saklanmaz, bütçe, anahtar ayrımı | `docs/API.md` Genel bilgiler; G4 | TC-ST-001..010 | `srv::main::cache_*` (6), `handler_serves_cached_hit_with_x_cache_and_does_not_cache_errors` | Kısmi (TC-ST-009 aynı anahtar yeniden ekleme EKSİK; TTL değerlerinin 600/21600 olduğu doğrudan sınanmıyor) |
| REQ-API-009 | Export: `format` csv/geojson (büyük/küçük harf duyarsız), diğeri 400; indirme üstbilgileri | `docs/API.md` export; G10 | TC-API-060..066 | `srv::main::export_format_parser_accepts_known_values_and_rejects_others`, `export_serves_cached_hit_as_csv_download` | Kısmi (GeoJSON HTTP yanıtı, CRLF, format-bbox öncelik sırası EKSİK) |
| REQ-API-010 | Calendar: `country` karakter ve uzunluk kuralı, `from ≤ to`, yıl kırpma, bbox ya hep ya hiç, 404/500/504 | `docs/API.md` calendar; G4 | TC-API-100..116, TC-DT-030..035 | `srv::main::calendar_*` (10), `srv::it::calendar_*` (4) | Kısmi (101, 107, 109, 116 EKSİK) |
| REQ-API-011 | `series`/`points`/`regions` parametre doğrulaması ve durum kodları (400/404/413/503/504) | `docs/API.md` series, points, regions | TC-API-200..219, TC-DT-036, 037 | `srv::archive_api::size_guard_rejects_oversized_region_before_parsing`, `request_timeout_returns_504_and_logs_warning` | **Kısmi, zayıf**: 200, 201, 203..208, 211..213 EKSİK. Uç noktalar HTTP düzeyinde neredeyse hiç sınanmıyor |
| REQ-API-012 | `points` örnekleme sonlanır, bağlı satırlar birlikte kalır, `limit` 0 → 400 (BUG-7) | `docs/API.md` points Örnekleme; BUG-7 | TC-API-209, 210, 214, 215 | `srv::archive_api::points_rejects_zero_limit_and_handles_limit_one_with_merged_hotspot`, `sample_links_*` (5) | Kapsandı |
| REQ-API-013 | Bilinmeyen `/api/*` JSON 404; SPA yedeği; `STATIC_DIR` yoksa panik yok; CORS | `docs/API.md`; `crates/server/CLAUDE.md`; K9 | TC-CORE-085 | `srv::main::router_returns_json_404_for_unknown_api_and_serves_cors_headers`, `spa_fallback_serves_index_for_unknown_paths_but_not_for_api`, `missing_static_dir_keeps_plain_404_without_panicking` | Kapsandı |
| REQ-API-014 | API anahtarı hiçbir yanıtta veya hata mesajında görünmez | `docs/API.md` Genel bilgiler | TC-FC-009, TC-FC-010 | `fc::client::api_key_is_redacted_from_urls_and_error_messages`, `srv::it::rejected_key_body_returns_502_without_leaking_the_key` | Kapsandı |
| REQ-API-015 | `/api/health` ve `/api/config` | `docs/API.md` | TC-CORE-086 | `srv::it::health_returns_ok_and_crate_version`, `srv::main::config_endpoint_reflects_state_config` | Kapsandı |
| REQ-API-016 | `compare` ve `stats` metrikleri (sayımlar, FRP özetleri, VZA işaret değişimi) | `docs/API.md` compare, stats; S3 notu | TC-CORE-083, 087 | `srv::main::compare_*`, `stats_*`, `srv::it::compare_shows_fewer_harmonized_records_than_raw_detections` | Kapsandı |
| REQ-API-017 | Türetilmiş seri önbelleği: kaynak önceliği, damga geçersizleşmesi, bozuk dosya, yazılamayan dizin | `docs/API.md` series Notlar | TC-ST-050..056 | `srv::archive_api::series_*` (9) | Kapsandı |
| REQ-API-018 | G4: `days` istekleri 10 s içinde yanıtlanır | G4 "Bitti sayılır" | (vaka yok) | Elle ölçüm (1,36 s en yavaş); otomatik yok | EKSİK |
| REQ-API-019 | `/api/sensitivity`: `radius_km` ve `window_min` listeleri (≤ 5 değer; `0 < r ≤ 10`; `0..=1440`), `matched` yarıçap/pencere ile azalmaz | `docs/API.md` sensitivity; S11 | TC-API-300..314 | `srv::it::sensitivity_rejects_bad_radius`, `sensitivity_accepts_boundary_values`, `sensitivity_rejects_oversized_grid_or_bad_window`, `sensitivity_rejects_missing_parameter`, `sensitivity_matched_never_decreases_with_radius_or_window` | Kısmi (tam 5 değer, 503, tekrarların atılması, `harmonize` ile tutarlılık EKSİK) |
| REQ-API-020 | `/api/series` anomalisi sensör geçişinden bağımsız ve Ω ile normalize; Terra-only aylar `null` (`/api/calendar` ile aynı kural) | Geçerleme bulgusu; `docs/API.md` CalendarCell kuralı | TC-API-220..223, TC-VAL-005, 020, 021, 024 | (yok) | **EKSİK, hata açık** (BUG-9, BUG-10: 138/138 bölgede +1,45σ kayma; 3818/3884 Terra-only satırda anomali) |
| REQ-API-021 | Paketli türetilmiş seri (`data/derived`) kodun şema sürümü ve yapılandırma özetiyle uyumlu | `docs/API.md` series Notlar (damga doğrulaması) | TC-API-224 | Kod tarafı damga uyuşmazlığını yakalar (`srv::archive_api::series_treats_v1_schema_with_mtime_secs_as_stale_and_recomputes`); **paketin kendisinin bayatlığını** yakalayan test yok | EKSİK (paket `schema_version: 2`, kod 3; sunucu yeniden hesaplıyor, bazı istekler 504) |

## 2. Harmonizasyon motoru

| REQ | Gereksinim | Kaynak | Vaka | Otomatik test (öne çıkan) | Durum |
|-----|------------|--------|------|---------------------------|-------|
| REQ-CORE-001 | Interscan dedup: Δt ≤ 8 s ve örtüşme > %30, en yüksek güven korunur, FRP azalması elle hesaplı | `crates/core/CLAUDE.md`; S1 | TC-CORE-001..011, 070..072 | `core::dedup::*` (eight/seven/outside, overlap_threshold, known_duplicate_set) | Kısmi (Δt=0, tam %30 EKSİK; belge-kod çelişkisi BUG-4: CLAUDE.md `<8`, kod `≤8`) |
| REQ-CORE-002 | Filo birleştirme: VIIRS 60 dk, MODIS 180 dk, 2 km, aynı sensör ve gündüz/gece kuralı | G2 ("Fleet rule"); `docs/API.md` | TC-CORE-012..021, 073 | `core::fleet::*` | Kısmi (tam 60/180 dk ve 2,0 km sınırı EKSİK) |
| REQ-CORE-003 | MODIS-VIIRS eşleştirme 2 km / 180 dk, bire bir, füzyonda VIIRS tercih | G2; `crates/core/CLAUDE.md` | TC-CORE-022..027, TC-DT-010..018 | `core::harmonize::*` (pair_*, exact_window_boundary, single_viirs_cannot_serve_two_modis) | Kısmi (2,0 km tam sınırı, güven birleştirme kombinasyonları, eşit skor kuralı EKSİK) |
| REQ-CORE-004 | VZA düzeltmesi çarpımsaldır, nadirde 1, kenara doğru azalmaz, geçersiz girdi reddedilir | `crates/core/CLAUDE.md`; S1 | TC-CORE-057, 058, 074, 075 | `core::vza::*` (21) | Kapsandı |
| REQ-CORE-005 | Takvim: yalnız tespitli aylar, anomali ≥ 3 hücre ve σ ≠ 0, popülasyon σ, UTC ay | `docs/API.md` CalendarCell; S1 | TC-CORE-050..054, 078, 088 | `core::calendar::*` (16) | Kısmi (tam 2 hücre sınırı ayrıca denenmedi; BUG-3 boş ay davranışı sabitlenmedi, TC-CORE-084 EKSİK) |
| REQ-CORE-006 | Ω: ≥ 3 ay, MODIS-only taban 1, Terra-only dönem dışlama | G3; `crates/core/CLAUDE.md` | TC-CORE-055, 056, 077 | `core::effort::*` (13) | Kapsandı |
| REQ-CORE-007 | `k_c`: yalnız VIIRS'siz hücrenin toplam FRP'sine, enlem bantları ve geçiş, geçersiz enlem kimlik | G8 kapanış notu; `crates/core/CLAUDE.md` | TC-CORE-028..031, TC-DT-001..005 | `core::calibration::*` (11), `core::calendar::total_frp_applies_completeness_only_when_cell_has_no_viirs_and_enabled` | Kısmi (kalibrasyon+hücre birleşik kural DT-004 ve geçiş ayı DT-005 EKSİK) |
| REQ-CORE-008 | Belirsizlik: `lower ≤ frp_corrected ≤ upper`, VZA ile genişleme, `1/√n`, `z > 0` | G9 "Bitti sayılır" | TC-CORE-059..064 | `core::uncertainty::*` (20+), `default_params_are_pinned_to_documented_values` (MODIS σ 0,266, VIIRS σ 0,08), `core::harmonize::fuse_matched_interval_ignores_modis_member_count` | Kapsandı (kod davranışı; parametrelerin doğruluğu REQ-SCI-003: VIIRS σ ölçülemedi, geçici varsayım) |
| REQ-CORE-009 | Yapılandırma: varsayılan değerler, doğrulama, TOML+env öncelik, bilinmeyen anahtar hata | G7 "Bitti sayılır" | TC-CORE-040..045 | `core::config::*` (16), `srv::pipeline_config::*` (8) | Kapsandı |
| REQ-CORE-010 | Export: 13 sütun, başlık, GeoJSON FeatureCollection, boş girdi hata değil, 10 000+ satır | G10 "Bitti sayılır"; S6 | TC-CORE-080..082 | `core::export::*` (13), `large_input_yields_one_line_per_hotspot` (10 000 satır) | Kapsandı (QGIS'te açılış elle; otomatik şema doğrulaması yok) |
| REQ-CORE-011 | H3 aday araması kaba kuvvetin süpersetidir | G2 | TC-CORE-089, TC-DT-017 | `core::h3index::*` (16) | Kapsandı |
| REQ-CORE-012 | Optimize uygulama referans uygulama ile bit düzeyinde aynı | G2 performans işi | TC-DT-018 | `timed_*_is_bit_identical_to_reference_across_seeds_and_configs` (3) | Kapsandı |
| REQ-CORE-013 | Yalnız tip 0 (bitki yangını) kayıtları işlenir | `docs/API.md` Harmonizasyon hattı | TC-FC-007 | `fc::parse::retain_wildfires_drops_volcanoes_and_static_sources` | Kapsandı |
| REQ-CORE-014 | Çekirdek saf hesaplamadır (I/O yok); tüm public işlevler birim testli | `crates/core/CLAUDE.md` | (vaka yok) | Kapsam raporunda `core` yüksek; "tüm public işlev" ifadesi doğrulanmadı | EKSİK (iddia ölçülmedi) |
| REQ-CORE-015 | VIIRS tarama bölgesi: `viirs_zone` sınırları 31,72°, 44,86°, 56,28° (iki işaret); `viirs_zone_layout` belge değerleriyle aynı | S11; `crates/core/src/dedup.rs` | TC-CORE-090, 091 | `core::dedup::viirs_zone_boundaries_on_both_signs`, `viirs_zone_layout_matches_aggregation_and_deleted_rows` | Kapsandı |

## 3. FIRMS istemcisi ve arşiv

| REQ | Gereksinim | Kaynak | Vaka | Otomatik test (öne çıkan) | Durum |
|-----|------------|--------|------|---------------------------|-------|
| REQ-FC-001 | CSV ayrıştırma: MODIS/VIIRS sütun farkları, bozuk satırda alan adıyla hata, panik yok | `crates/firms-client/CLAUDE.md`; G1 | TC-FC-005..008 | `fc::parse::*` (25) | Kapsandı |
| REQ-FC-002 | Hız sınırı 5000/10 dk, eşzamanlılık sınırı, 429/5xx yeniden deneme (en çok 2, `Retry-After` eklenir), 4xx ve ağ hatası yeniden denenmez | `docs/API.md` Zaman penceresi; `client.rs` | TC-ST-030..042 | `fc::limiter::*` (12), `fc::client::retry_after_header_is_added_to_the_backoff`, `persistent_5xx_exhausts_retries_with_growing_backoff_then_fails`, `client_errors_are_not_retried`, `connection_failure_is_reported_with_redacted_url_and_billed_once`, `srv::it::transient_503_is_retried_and_both_attempts_are_counted` | Kapsandı (6400ff4; `start_paused` sanal zaman) |
| REQ-FC-003 | Alan önbelleği: isabet işlem harcamaz, SP süresiz, NRT TTL (sınır dahil), hata ve doğrulanmamış gövde saklanmaz, boş yanıt isabet, atomik yazma | K5; `crates/firms-client/CLAUDE.md` | TC-FC-014..019 | `fc::area_cache::*` (15+), `fc::client::window_walks_miss_stored_hit_without_a_second_request`, `rejected_and_failed_bodies_are_never_cached`, `empty_valid_response_is_cached_as_a_hit` | Kapsandı |
| REQ-FC-004 | Availability: kaynak seçimi (SP/NRT), SP-NRT sınırında boşluk yok, diske yedek, başarısızlık saklanmaz, **güncel tutulur** | `crates/firms-client/CLAUDE.md`; BUG-8 | TC-ST-020..026, TC-FC-011..013 | `fc::availability::*` (15+), `fc::client::availability_is_fetched_once_then_served_from_memory`, `failed_availability_fetch_is_not_cached_and_the_next_call_retries`, `availability_success_is_persisted_and_failure_falls_back_to_that_file`, `availability_reflects_newer_firms_data_on_a_later_call` (`#[ignore]`) | Kısmi: yenileme gereksinimi `#[ignore]` testle sabit ve `--ignored` ile **başarısız** (BUG-8 açık); uçtan uca gün sınırı vakası (TC-ST-025 ikinci yarısı) ve yedekten sonra yenileme (TC-ST-024) EKSİK |
| REQ-FC-005 | Arşiv salt okunur, eksik dosya indirme tetiklemez; kapsam yılları; zip-slip engeli | `docs/API.md` calendar; `archive.rs` | TC-FC-020..023 | `fc::archive::*` (12), `fc::archive_seed::*` (9) | Kapsandı |
| REQ-FC-006 | İstek pencereleri kaynak başına ≤ 5 gün, maliyet = gün | `docs/API.md` Zaman penceresi | TC-FC-001..004, TC-API-052 | `fc::day_range::*` (15), `fc::client::splits_multi_week_range_into_max_five_day_windows_and_cost` | Kapsandı (`new_accepts_exact_limits_and_rejects_outside_with_message`) |

## 4. Frontend

| REQ | Gereksinim | Kaynak | Vaka | Otomatik test | Durum |
|-----|------------|--------|------|---------------|-------|
| REQ-WEB-001 | Bölge geometrisi ve projeksiyon geçerli, antimeridyen doğru | R2 | TC-WEB-001, 002 | `web:geo`, `web:worldGeo`, `web:countryPick`, `web:globe`, `web:globeTiles`, `web:viewport`, `web:pointIndex` | Kapsandı |
| REQ-WEB-002 | Radyal takvim hesapları, durum deposu, güven süzgeci (URL hash), alıp-sayım tutarlılığı, hash yönlendirme, tarama boşluğu notu, mock veri | R3, R4; C8-C12 | TC-WEB-003, 008 | `web:radialCalendar`, `web:charts-link`, `web:confidenceFilter`, `web:receiptCounts`, `web:route`, `web:swathGap`, `web:mocks`, `web:footprint` | Kısmi (bileşenin kendisi sınanmıyor) |
| REQ-WEB-003 | Çizim performansı eşikleri | R2 | TC-WEB-004 | `web:bench`, `web:scene` | Kapsandı (eşikler CI makinesinde kararsız olabilir) |
| REQ-WEB-004 | `api.ts` wire formatını doğru tiplere çevirir; `warnings` ve `effort.reliable` kullanıcıya gösterilir; hata gövdesi okunur | `docs/API.md` harmonize Notlar; R4 "API hatası kullanıcıya mesaj" | TC-WEB-005 | (yok) | EKSİK |
| REQ-WEB-005 | Arayüz akışları: toggle, takvim etkileşimi, erişilebilirlik | R3, R5 "Bitti sayılır" | TC-WEB-007 | (yok, E2E yok) | EKSİK |
| REQ-WEB-006 | `csv.ts`, `urlState.ts`, `format.ts` | R7, K2 | TC-WEB-006 | `web:confidenceFilter` yalnız `urlState` güven süzgeci hash'i; `csv.ts`, `format.ts` testsiz | Kısmi |

## 5. Süreç ve bilimsel gereksinimler

| REQ | Gereksinim | Kaynak | Vaka | Otomatik test / kanıt | Durum |
|-----|------------|--------|------|-----------------------|-------|
| REQ-NFR-001 | Her push'ta CI: check, clippy `-D warnings`, test, coverage kapısı (%90), web build; Rust 1.100'e sabit, iş zaman aşımları | S5 "Bitti sayılır" | (CI) | `.github/workflows/ci.yml` | Kısmi (tanımlı; GitHub'da yeşil çalıştığı **doğrulanmadı**, depo özel; S5 açık) |
| REQ-NFR-002 | Satır kapsamı ≥ %90 (CI kapısı) | TEST_PLANI §9 C3 | (CI) | `cargo llvm-cov --fail-under-lines 90`; ölçülen %94,7 (bölge %95,3, fonksiyon %94,5) | Kısmi (genel geçiyor; dosya bazlı değerler 6400ff4 sonrası yeniden ölçülmedi; dal kapsamı ölçülmüyor; yüksek kapsam BUG-9/10'u yakalamadı) |
| REQ-NFR-003 | Clippy temiz | S1 "Doğrulama" | (CI) | `cargo clippy ... -D warnings` | Kapsandı |
| REQ-NFR-004 | Testler tekrarlanabilir (gerçek saat kullanılmaz) | F.I.R.S.T. (Hafta 4) | TC-ST-025 | 9b0ed86: saate bağlı olmayan testler sabit gün kullanıyor. `FirmsClient::fetch_all` ve `fetch_recent` `Utc::now()` okuyor (`client.rs`) | Kısmi (enjekte edilebilir `today` yok; backend değişikliği, açık iyileştirme) |
| REQ-SEC-001 | Bağımlılıklarda bilinen zafiyet yok | OWASP/ISTQB iyi uygulama | (yok) | `cargo audit`, `npm audit` yok | EKSİK |
| REQ-SCI-001 | 2000-bugün seri sensör geçişlerinde (2002, 2012, 2018, 2023) yapay sıçrama göstermez; `frp_per_detection_eq` 2012'de sürekli | G3, G8 "Bitti sayılır"; K6, K7 | TC-VAL-020..023 | Sentetik: `synthetic_sensor_transition_eliminates_artificial_jump_and_false_anomaly`. Gerçek veri (seri yolu): **Kaldı** (4,68× sıçrama, +1,45σ kayma); FRP alanları ve 2018/2023 yapılmadı | EKSİK (geçerleme başarısız veya yapılmadı) |
| REQ-SCI-002 | Temmuz-Ağustos 2021 Antalya/Muğla yangınları takvimde anomali olarak belirgin; bilinen büyük olaylar görünür | S7 "Bitti sayılır" | TC-VAL-001..015 | Betikle geçerleme: 15 olaydan 10 geçti, 5 kaldı (Türkiye 2021-08 z 1,44; Yunanistan 2007-08 z 0,52). Antalya-Muğla bbox z 4,88 (önceki ölçüm). Otomatik test yok | Kısmi (elle) |
| REQ-SCI-003 | Belirsizlik parametreleri ölçülmüş/yayımlanmış: MODIS σ 0,266 (Freeborn 2014a); VIIRS σ; aralıklar ≈ %68 kapsar | G9 açık notu; S9; `docs/API.md` Belirsizlik | TC-VAL-040, 041, 043, TC-CORE-064 | MODIS σ yayımlanmış değerden; **VIIRS σ ölçülemedi** (SNPP-NOAA-20 ≈ 50 dk, 0 çift; 0,08 geçici); kapsama oranı yapılmadı; varsayılanlar test ile sabitlendi | Kısmi (parametre sabit, doğruluğu doğrulanmadı) |
| REQ-SCI-004 | `k_c` katsayıları bağımsız veriyle doğrulanmıştır | G8 kapanış notu | TC-VAL-004 | Geliştirici hold-out (2020), bağımsız tekrar yok | Kısmi |
| REQ-SCI-005 | Veri bütünlüğü: arşiv SHA-256, seri iç tutarlılığı (toplamlar, z kuralları) | `make verify-data`; `docs/API.md` series | TC-VAL-025, 027 | Betik ve SHA-256 doğrulaması (10095/10095 OK); otomatik test yok (CI'da yok) | Kısmi (elle geçti; CI'a alınmamış) |

## 6. Gereksinim kapsama özeti

| Durum | Sayı |
|-------|------|
| Kapsandı | 26 |
| Kısmi | 24 |
| EKSİK | 8 |
| Toplam REQ | 58 |

## 7. Boşluk özeti (risk sırasıyla)

Risk = etki × olasılık; etki, kullanıcıya yanlış sonuç gösterme veya servis kaybı ağırlıklıdır. Sıra bu turda değişti: iki gerçek bilimsel hata geçerlemeyle bulundu.

| Sıra | Boşluk | Risk nedeni | İlgili REQ / vaka | Önerilen eylem |
|------|--------|-------------|--------------------|----------------|
| 1 | **BUG-9 ve BUG-10 açık, ve hiçbir otomatik test bunları tanımıyor** (`/api/series` anomalisi Ω'suz, Terra-only aylara anomali; Yunanistan 2007-08 z 0,52) | Ürünün ana göstergesi (takvim anomalisi) 2013 sonrası her ayı "yüksek" gösteriyor; 600 yeşil test buna kördü | REQ-API-020, REQ-SCI-001, TC-API-220..223, TC-VAL-005, 020, 021, 024 | Önce belirtimi ve kabul testlerini yaz (TC-API-220..223), sonra düzeltme; `docs/API.md` serisi bölümüne geçici uyarı |
| 2 | **Bilimsel geçerlemenin yarısı yapılmadı**: FRP tabanlı süreklilik (`/api/calendar`), 2018 ve 2023 geçişleri, aralık kapsama oranı; VIIRS σ ölçülemedi (geçici 0,08) | Ürünün temel iddiası ("sensör geçişlerinden bağımsız seri") yalnız sayı tabanı için sınandı ve kaldı | REQ-SCI-001, 003, TC-VAL-022, 023, 040, 043 | `/api/calendar` çıktısıyla TC-VAL-022; VIIRS σ için yayımlanmış kaynak ya da farklı ölçüm yöntemi |
| 3 | **BUG-8 açık**; yalnız `#[ignore]` testle sabit, uçtan uca gün sınırı vakası yok, `today` enjekte edilemiyor | Demo günlerce açık kalırsa en yeni veri sessizce (200 + uyarı) gelmez | REQ-FC-004, REQ-NFR-004, TC-ST-024, 025 | Enjekte edilebilir `today` (Görkem), TTL'li yenileme, ignore'un kaldırılması |
| 4 | **Bayat paketli seri** (`schema_version` 2, kod 3): sunucu yeniden hesaplıyor, bazı istekler 504; bunu yakalayan test/CI adımı yok | Kullanıcıya görünen zaman aşımı; yapılandırma değişince tekrarlanır (R-14) | REQ-API-021, TC-API-224 | Paketi yeniden üret; paketin `schema_version` ve `config_hash` değerini koddakiyle karşılaştıran test ya da CI adımı |
| 5 | **Arşiv uç noktaları HTTP düzeyinde doğrulanmıyor** (`month`, `limit` üst sınırı, `year`, bilinmeyen bölge, eksik parametre) | Arayüzün ana veri yolu; BUG-7 aynı alandan çıktı | REQ-API-011, TC-API-200..213 | Yönlendirici üzerinden rstest tablosu (harmonize'daki gibi) |
| 6 | **502 kuralı belge ile kod arasında çelişiyor** | Bir kaynak boş+başarılı, diğeri başarısızsa 502; belge "tüm istekler başarısız" der | REQ-API-006, TC-DT-027 | Karar ver (belge veya kod), vaka yaz |
| 7 | **Frontend sözleşme dönüşümü (`api.ts`) ve arayüz testsiz**, E2E yok | `warnings` ve `effort.reliable` gösterilmezse yanlış güven; API değişikliği sessiz kalır | REQ-WEB-004..006 | `api.ts` için fixture tabanlı sözleşme testi; sonra tarayıcı testi |
| 8 | **Eşik tam sınırları** (60/180 dk, 2,0 km, %30 örtüşme, Δt=0) ve BUG-4 belge çelişkisi | Off-by-one; sessiz kayma, küçük etki | REQ-CORE-001..003, TC-CORE-004/008/014/016/019/026 | Tek satırlık sınır testleri; kararı `crates/core/CLAUDE.md`'ye yaz |
| 9 | **CI doğrulanmadı** (depo özel), dal kapsamı ölçülmüyor, dosya bazlı kapsam yeniden ölçülmedi, `cargo audit`/`npm audit` yok | Yanlış güven, bilinmeyen zafiyet | REQ-NFR-001, 002, REQ-SEC-001 | Actions sonucunu gör; nightly dal kapsamı; audit işi |
| 10 | **Boş ay davranışı (BUG-3) ve sayı tabanlı anomali (BUG-6)** testle sabitlenmedi | Belgeli sınırlama; bilimsel yanlış okuma | TC-CORE-084 | Mevcut davranışı sabitleyen test, karar sonrası güncelle |

Kapanan boşluklar (önceki turdan): FIRMS yolu hata dalları (429, `Retry-After`, deneme tükenmesi, ağ hatası), availability yedek ve yeniden deneme davranışı, NRT TTL sınırı, `DayRange` sınırları (6400ff4); S6 sınır testleri (`78785f2`).

Çelişkiler (kod ya da belge ile görev tanımı arasında): ayrıntı final raporunda. Özet: `GOREVLER.md` S2 maddesi "anahtar yoksa 500" der (kod ve `docs/API.md` 503); `.env.example` yorumu 500 der; `GOREVLER.md` G7 eşleştirme süresini "5 dk" yazar (kod 180 dk); `crates/core/CLAUDE.md` "Δt<8s" der (kod ≤ 8); `docs/MIMARI.md` çekirdek modüllerinin yalnız beşini listeler; `integration_tests.rs` içinde BUG-2 için bayat yorum ("plain text") var; `docs/API.md` 502 koşulu koddan farklı.
