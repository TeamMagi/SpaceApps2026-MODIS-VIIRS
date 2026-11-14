# pyrosync-server — Axum Web Sunucusu

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- API endpoint'leri `/api/` prefix'i ile başlar.
- Statik dosyalar `STATIC_DIR` env var ile belirlenen dizinden servis edilir (varsayılan: `web/dist`).

## API Endpoint'leri

| Endpoint | Metod | Açıklama |
|----------|-------|----------|
| `/api/health` | GET | Sağlık kontrolü, versiyon bilgisi |
| `/api/harmonize` | GET | Harmonize edilmiş veri (query params: west, south, east, north, days) |
| `/api/availability` | GET | FIRMS kaynaklarının erişilebilir tarih aralıkları |
| `/api/compare` | GET | Ham ve harmonize veri karşılaştırma istatistikleri (query params: /api/harmonize ile aynı) |
| `/api/stats` | GET | Genel istatistikler, sensör kırılımı, pipeline metrikleri (query params: /api/harmonize ile aynı) |

## Konfigürasyon (Env Vars)

| Değişken | Varsayılan | Açıklama |
|----------|-----------|----------|
| `FIRMS_API_KEY` | — | NASA FIRMS API anahtarı (zorunlu) |
| `PORT` | `3001` | Sunucu portu |
| `STATIC_DIR` | `web/dist` | Frontend build çıktı dizini |
| `RUST_LOG` | `pyrosync=info` | Log seviyesi |
| `CACHE_BUDGET_MB` | `128` | Response cache bellek bütçesi (MB). `0` cache'i devre dışı bırakır |
| `REQUEST_TIMEOUT_SECS` | `300` | İstek zaman aşımı (saniye). `0` zaman aşımını devre dışı bırakır |
| `CORS_ALLOWED_ORIGINS` | — | Virgülle ayrılmış izinli origin listesi. Tanımsızsa CORS permissive olur |

## Teknik Notlar

- `tower_http::services::ServeDir` ile SPA fallback sağlanır; `/api/*` altındaki bilinmeyen yollar ise JSON `404` döner.
- `CORS_ALLOWED_ORIGINS` tanımlıysa yalnızca belirtilen origin'lere (`GET`, `content-type`) izin verilir; tanımsızsa `CorsLayer::permissive()` kullanılır ve başlangıçta uyarı loglanır.
- `/api/harmonize`, `/api/compare` ve `/api/stats` yanıtları bellek bütçeli response cache kullanır ve `X-Cache: HIT` / `MISS` header'ı döner.
- Response cache iki seviyeli TTL uygular: `days` (NRT) istekleri 600 sn, `start`/`end` (arşiv) istekleri 21600 sn (6 saat).
- Docker'da `STATIC_DIR=/app/static` olarak ayarlanır.
- Port 3001 üzerinde `0.0.0.0` dinler (container erişimi için).
