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

## Teknik Notlar

- `tower_http::services::ServeDir` ile SPA fallback sağlanır.
- `CorsLayer::permissive()` sadece geliştirme için; production'da kısıtlanmalıdır.
- Docker'da `STATIC_DIR=/app/static` olarak ayarlanır.
- Port 3001 üzerinde `0.0.0.0` dinler (container erişimi için).
