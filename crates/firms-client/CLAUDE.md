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
| `area_cache.rs` | Area API pencerelerinin diskte önbelleği (`{dir}/{KAYNAK}/{başlangıç}_{gün}d_{bbox}.csv`) ve `availability.csv` |

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
- `with_area_cache` ile önbellek açıksa cache hit istek atmaz, limiter'a girmez ve transaction harcamaz (`transactions_spent` yalnızca gerçek istekleri sayar). Boş (olaylı olmayan) geçerli yanıt da 0 baytlık dosya olarak önbelleğe alınır; hata ve doğrulamadan geçmeyen gövdeler asla yazılmaz.
- SP pencereleri süresizdir; NRT pencereleri dosya değişim zamanına göre `nrt_ttl` süresince geçerlidir (`0` = NRT hiç önbellekten sunulmaz). Yazma hataları yalnızca loglanır.
- Availability başarılı çekilince `availability.csv` olarak saklanır; çekim başarısız olursa bu dosya uyarı loglanarak kullanılır (ağsız çalışma).
