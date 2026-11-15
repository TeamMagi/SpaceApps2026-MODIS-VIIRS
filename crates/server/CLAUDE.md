# pyrosync-server — Axum Web Sunucusu

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- API endpoint'leri `/api/` prefix'i ile başlar.
- Statik dosyalar `STATIC_DIR` env var ile belirlenen dizinden servis edilir (varsayılan: `web/dist`).

## API Endpoint'leri

| Endpoint | Metod | Açıklama |
|----------|-------|----------|
| `/api/health` | GET | Sağlık kontrolü, versiyon bilgisi |
| `/api/config` | GET | Etkin `PipelineConfig` (harmonizasyon eşikleri) JSON olarak |
| `/api/harmonize` | GET | Harmonize edilmiş veri (`frp_lower`/`frp_upper` + üst düzey `uncertainty: { confidence, note }`; query params: west, south, east, north, days) |
| `/api/availability` | GET | FIRMS kaynaklarının erişilebilir tarih aralıkları |
| `/api/compare` | GET | Ham ve harmonize veri karşılaştırma istatistikleri (query params: /api/harmonize ile aynı) |
| `/api/stats` | GET | Genel istatistikler, sensör kırılımı, pipeline metrikleri (query params: /api/harmonize ile aynı) |
| `/api/calendar` | GET | Arşivden tarihsel aylık yanma takvimi (`total_frp`, `total_frp_lower`/`total_frp_upper`, `frp_per_detection_eq`, `mean_frp*` + `uncertainty: { confidence, note }`; query params: country, from, to, opsiyonel bbox) |
| `/api/export` | GET | Harmonize edilmiş hotspot'ları dosya olarak indirir (query params: /api/harmonize ile aynı + `format=csv\|geojson`, varsayılan csv). `region` parametresi varsa arşiv modu: `/api/points` ile aynı parametreler + `year`, opsiyonel `month`, `view=harmonized\|raw`, limitsiz (500000 tespit sınırı, aşılırsa 413), BOM'lu CSV veya GeoJSON, `FIRMS_API_KEY` gerekmez |

## Konfigürasyon (Env Vars)

| Değişken | Varsayılan | Açıklama |
|----------|-----------|----------|
| `FIRMS_API_KEY` | — | NASA FIRMS API anahtarı (zorunlu) |
| `ARCHIVE_CACHE_DIR` | `.cache/firms-archive` | FIRMS ülke arşivi önbellek dizini |
| `PORT` | `3001` | Sunucu portu |
| `STATIC_DIR` | `web/dist` | Frontend build çıktı dizini |
| `RUST_LOG` | `pyrosync=info` | Log seviyesi |
| `CACHE_BUDGET_MB` | `128` | Response cache bellek bütçesi (MB). `0` cache'i devre dışı bırakır |
| `REQUEST_TIMEOUT_SECS` | `300` | İstek zaman aşımı (saniye). `0` zaman aşımını devre dışı bırakır |
| `AREA_CACHE_DIR` | `.cache/firms-area` | FIRMS Area API pencere önbelleği dizini (cache hit transaction harcamaz). Boş değer devre dışı bırakır |
| `AREA_CACHE_NRT_TTL_SECS` | `600` | Önbellekteki NRT penceresinin geçerlilik süresi (sn). SP pencereleri süresizdir; `0` NRT'yi önbellekten hiç sunmaz |
| `CORS_ALLOWED_ORIGINS` | — | Virgülle ayrılmış izinli origin listesi. Tanımsızsa CORS permissive olur |
| `PYROSYNC_CONFIG` | — | Pipeline eşikleri için TOML dosya yolu. Verilirse dosya zorunludur; verilmezse çalışma dizinindeki `pyrosync.toml` varsa kullanılır. Geçersiz konfigürasyon başlangıçta hata ile durdurur |
| `PYROSYNC_DEDUP_MAX_TIME_DIFF_SECS` | `8` | Interscan duplikat zaman farkı (sn); TOML'u ezer |
| `PYROSYNC_DEDUP_MIN_OVERLAP_RATIO` | `0.30` | Interscan duplikat minimum örtüşme oranı |
| `PYROSYNC_FLEET_VIIRS_WINDOW_SECS` | `3600` | VIIRS filo birleştirme penceresi (sn) |
| `PYROSYNC_FLEET_MODIS_WINDOW_SECS` | `10800` | MODIS filo birleştirme penceresi (sn) |
| `PYROSYNC_FLEET_RADIUS_KM` | `2.0` | Filo birleştirme yarıçapı (km) |
| `PYROSYNC_MATCH_RADIUS_KM` | `2.0` | MODIS/VIIRS eşleştirme yarıçapı (km, en fazla 10) |
| `PYROSYNC_MATCH_WINDOW_SECS` | `10800` | MODIS/VIIRS eşleştirme zaman penceresi (sn) |
| `PYROSYNC_MATCH_SIMULTANEOUS_WINDOW_SECS` | `1200` | Radyometrik kestirim için eşzamanlı Matched alt kümesi penceresi (sn) |
| `PYROSYNC_VZA_A`, `PYROSYNC_VZA_B`, `PYROSYNC_VZA_C` | `0.2017`, `0.0168`, `1.0` | MODIS VZA düzeltme polinomu `R_rel(θ)` katsayıları (Zheng et al. 2026 Eq. 9 & Fig. 4 nadir normalize) |
| `PYROSYNC_STATIC_MASK_ENABLED`, `PYROSYNC_STATIC_MASK_MIN_ACTIVE_MONTHS` | `true`, `10` | Kalıcı sıcak hücre (gaz meşalesi) maskesi: bir yıl içinde ≥ N farklı ayda tespit olan H3 r7 hücresi o yıl için elenir |
| `PYROSYNC_SAMA_ENABLED` | `false` | SAMA kutusundaki gece VIIRS tespitlerini eler (C2 arşivinde fazla gürültü ölçülmediği için kapalı) |
| `PYROSYNC_TROPICAL_SWATH_GAP_COMPENSATION` | `false` | 30°G–30°K MODIS swath boşluğu için VIIRS-siz takvim `total_frp` telafisini açar |
| `CONFORMAL_TABLE` | `data/conformal.json` | Mondrian conformal kantil tablosu; dosya yoksa veya parmak izi etkin konfigürasyonla eşleşmezse `/api/calendar` `conformal` alanı eklenmez |

## Teknik Notlar

- `ServeDir` statik dosyaları sunar; bilinmeyen API dışı yollar `ServeFile` ile `index.html`'e düşer, böylece derin bağlantılar ve F5 çalışır. `index.html` yoksa fallback uyarı loglanarak devre dışı kalır ve bu yollar `404` döner; `/api/*` altındaki bilinmeyen yollar ise JSON `404` döner.
- `CORS_ALLOWED_ORIGINS` tanımlıysa yalnızca belirtilen origin'lere (`GET`, `content-type`) izin verilir; tanımsızsa `CorsLayer::permissive()` kullanılır ve başlangıçta uyarı loglanır.
- `/api/harmonize`, `/api/compare` ve `/api/stats` yanıtları bellek bütçeli response cache kullanır ve `X-Cache: HIT` / `MISS` header'ı döner.
- Response cache iki seviyeli TTL uygular: `days` (NRT) istekleri 600 sn, `start`/`end` (arşiv) istekleri 21600 sn (6 saat).
- `/api/calendar` FIRMS ülke arşivini kullanır, transaction harcamaz ve `FIRMS_API_KEY` gerektirmez; buna karşılık arşiv NOAA-21 taşımadığı için (NOAA-20 `viirs-jpss1` olarak arşivde var) NRT endpoint'lerinden daha küçük bir filo kapsar.
- `/api/series` yıllık statik maskeyi tüm yıldan bir kez kurar ve aylık sayımlarla paylaşır; `years[]` ve `/api/points` `stages` alanları `static_masked` / `sama_removed` sayılarını taşır. Bu alanlar eklendiği için `DERIVED_SCHEMA_VERSION = 3` oldu. Ayrıca `years[]` ve `/api/points` `stages` `viirs_aggregated` (bir MODIS ayak izinde tek Matched kayıtta toplanan ek VIIRS gözlemleri) taşır; aylık `months[]` `mean_frp*` alanları (v4) ile birlikte `DERIVED_SCHEMA_VERSION = 5` oldu; toplam `Matched` kayıtlarının belirsizlik aralığı karesel birleştirmeye geçince (aylık `mean_frp_*` değerleri değişti, config hash'i değişmedi) `6` oldu.
- Global sensör kapsamı (`coverage.json` + `coverage.meta.json`) bellek → `DERIVED_CACHE_DIR` → `BUNDLED_DERIVED_DIR` → arşiv taraması sırasıyla önbelleklenir; damga `DERIVED_SCHEMA_VERSION` ve tüm arşiv dosyalarının sıralı `(path, bytes)` özetidir (`PipelineConfig`'e bağlı değildir).
- `pyrosync-server precompute` bölgelerden önce kapsamı da yazılabilir diske yazar; `make bundle-derived` bunu `data/derived/` altına kopyalar.
- Docker'da `STATIC_DIR=/app/static` olarak ayarlanır.
- Port 3001 üzerinde `0.0.0.0` dinler (container erişimi için).
