# PyroSync Mimari Dokümantasyonu

## Genel Bakış

PyroSync, NASA FIRMS API'den gelen MODIS ve VIIRS uydu yangın hotspot verilerini
harmonize ederek birleşik bir yanma aktivitesi takvimi oluşturur.

## Dizin Yapısı

```
SpaceApps2026-MODIS-VIIRS/
├── crates/                  # Rust workspace
│   ├── core/                # Harmonizasyon motoru
│   │   └── src/
│   │       ├── models.rs    # Veri modelleri (Hotspot, Sensor, Confidence)
│   │       ├── dedup.rs     # İnterscan duplikat temizleme
│   │       ├── vza.rs       # VZA açı düzeltmesi
│   │       ├── harmonize.rs # Ana harmonizasyon pipeline
│   │       └── calendar.rs  # Yanma takvimi oluşturma
│   ├── firms-client/        # NASA FIRMS API istemcisi
│   │   └── src/
│   │       ├── client.rs    # HTTP istemci (reqwest)
│   │       └── parse.rs     # CSV ayrıştırma
│   └── server/              # Axum web sunucusu
│       └── src/
│           └── main.rs      # API endpoint'leri + statik dosya servisi
├── web/                     # React frontend (SWC)
│   └── src/
│       ├── components/      # UI bileşenleri
│       │   ├── Layout/      # Sayfa düzeni (AppShell)
│       │   ├── Toggle/      # Raw/Harmonized geçiş
│       │   └── Stats/       # İstatistik kartları
│       ├── hooks/           # Custom React hooks (useApi)
│       ├── services/        # API çağrıları
│       ├── types/           # TypeScript tip tanımları
│       └── mocks/           # Örnek veri setleri
├── docs/                    # Proje dokümantasyonu
├── tools/                   # Yardımcı scriptler
├── Dockerfile               # Multi-stage Docker build
├── docker-compose.yml       # On-premise dağıtım
├── Makefile                 # Build otomasyon komutları
└── Cargo.toml               # Rust workspace konfigürasyonu
```

## 7 Aşamalı Harmonizasyon Pipeline

```
1. Ingest        → FIRMS API'den ham veri çekme (MODIS + VIIRS); type != 0 atılır
   Ön filtre     → (ops.) SAMA gece VIIRS filtresi, kalıcı sıcak hücre maskesi
2. Dedup         → İnterscan duplikatları temizleme (|Δt|<8s, overlap>30%)
3. VZA Correction → MODIS FRP'yi görüş açısına göre düzeltme
4. Spatial Harm. → H3 grid üzerinde mekansal birleştirme
5. Cross-sensor  → Sensörler arası eşleştirme (2km, 5dk)
6. Quality Fusion → Kalite-öncelikli birleştirme (VIIRS tercihli)
7. Calendar      → Aylık yanma takvimi çıktısı
```

### Ön filtre: kalıcı sıcak hücre maskesi ve SAMA

FIRMS C2 aylık dosyalarında `type` alanı sürüm 1-2'de tutarsızdır (VIIRS C2 375 m User Guide v1.2, Bölüm 6.1); belirgin statik kaynaklar sıkça `type 0` olarak gelir. `static_mask.rs`, bir H3 r7 hücresini (~5 km²) aynı takvim yılında ≥ `min_active_months` farklı ayda tespit içeriyorsa o yıl için eler. Eşik, `crates/firms-client/examples/static_mask_check.rs` (`tools/static-mask-check.sh`) ile 2013-2024 arşivinden ölçüldü: FIRMS'in kendi `type 1/2` hücreleri doğruluk için alt sınır kabul edildi.

| Eşik (ay) | Nijerya | Irak | Cezayir | Rusya | ABD | Endonezya | Avustralya | Portekiz |
|---|---|---|---|---|---|---|---|---|
| Elenenlerin FIRMS statik hücrede olma payı, 8 ay | %86 | %89 | %95 | %91 | %79 | %69 | %62 | %100 |
| aynı, 10 ay (varsayılan) | %89 | %96 | %96 | %93 | %89 | %89 | %84 | %100 |
| Elenen `type 0` payı, 10 ay | %0.21 | %13.4 | %19.7 | %0.98 | %1.17 | %0.80 | %0.06 | %0.05 |

SAMA gece filtresi (`sama.rs`, kutu 110°B–11°D, 55°G–7°K; VFIRE375 SPA v1.9) varsayılan kapalıdır: C2 algoritması SAMA düşük güven piksellerini zaten çıkarır ve arşivde kutu içi gece VIIRS tespitleri (Brezilya, Bolivya, Paraguay, Arjantin 2018-2020, tüm VIIRS'in %22-36'sı) gündüz ve kutu dışı geceye göre daha az yalıtıktır (%1.0-2.6, gündüz %3.9-7.2, Meksika gece %3.7). MODIS'e uygulanmaz (anomali yalnız VIIRS I4 için belgelenmiştir; MODIS gece yalıtık oranı %0.4-1.1).

## Teknoloji Stack

| Katman    | Teknoloji                     |
|-----------|-------------------------------|
| Backend   | Rust, Axum, Tokio, Reqwest    |
| Frontend  | React 19, Vite 6, SWC        |
| Veri      | NASA FIRMS API (CSV)          |
| Deploy    | Docker multi-stage, Compose   |
| Test      | cargo test, Vite build check  |

## API Endpoint'leri

- `GET /api/health` — Sağlık kontrolü
- `GET /api/harmonize?west=&south=&east=&north=&days=` — Harmonize edilmiş veri

## Veri Akışı

```
FIRMS API (CSV) → firms-client (parse) → core (harmonize) → server (JSON) → web (React)
```
