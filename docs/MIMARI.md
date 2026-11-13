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
1. Ingest        → FIRMS API'den ham veri çekme (MODIS + VIIRS)
2. Dedup         → İnterscan duplikatları temizleme (|Δt|<8s, overlap>30%)
3. VZA Correction → MODIS FRP'yi görüş açısına göre düzeltme
4. Spatial Harm. → H3 grid üzerinde mekansal birleştirme
5. Cross-sensor  → Sensörler arası eşleştirme (2km, 5dk)
6. Quality Fusion → Kalite-öncelikli birleştirme (VIIRS tercihli)
7. Calendar      → Aylık yanma takvimi çıktısı
```

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
