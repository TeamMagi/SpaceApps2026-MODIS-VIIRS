# pyrosync-firms — NASA FIRMS API İstemcisi

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- API anahtarı hard-code edilmez, `FIRMS_API_KEY` env var üzerinden okunur.
- Bu crate `pyrosync-core`'a bağımlıdır: `pyrosync-core = { path = "../core" }`.

## Modül Sorumlulukları

| Modül | Görev |
|-------|-------|
| `client.rs` | HTTP istemci (reqwest), `fetch_all()` ile MODIS + 3 VIIRS kaynağını paralel çeker |
| `parse.rs` | FIRMS CSV formatını parse eder, MODIS ve VIIRS sütun farklılıklarını yönetir |

## API Kaynakları

```
MODIS_NRT        → Terra + Aqua (birleşik)
VIIRS_SNPP_NRT   → Suomi NPP
VIIRS_NOAA20_NRT → NOAA-20
VIIRS_NOAA21_NRT → NOAA-21
```

Base URL: `https://firms.modaps.eosdis.nasa.gov/api/area/csv`

## Teknik Notlar

- `tokio::try_join!` ile paralel fetch yapılır.
- CSV parse sırasında MODIS'te `confidence` string (l/n/h), VIIRS'te numerik olabilir.
- Network hatalarında `anyhow::Error` döndürülür, loglama `tracing` ile yapılır.
