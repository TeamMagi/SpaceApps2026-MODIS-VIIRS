# Örnek (demo) veri seti

Frontend ve grafik component'leri backend, API anahtarı ve ağ olmadan geliştirilebilsin diye üç bölge için 2020–2025 FIRMS verisi: bölge başına ham MODIS CSV, ham VIIRS CSV ve harmonize edilmiş JSON.

Veri, yerel FIRMS ülke arşivinden (`<slug>/<yıl>/<slug>_<yıl>_<Ülke>.csv`, SP işleme) `sample_region` örneğiyle kesilir. Arşive hiçbir şey yazılmaz.

## Klasör yapısı

```
data/sample/
├── README.md
├── turkey/       modis.csv  viirs.csv  harmonized.json
├── california/   modis.csv  viirs.csv  harmonized.json
├── australia/    modis.csv  viirs.csv  harmonized.json
└── *.csv         (5 eski NRT/SP örnek dosyası, bu görevden önce vardı; değişmedi)
```

## Bölgeler

Üç bölge de **tam ülke/eyalet değil, alt bölgedir**. Nedeni boyuttur (aşağıda "Bilinçli sınırlamalar").

| Klasör | Bölge | bbox (`batı,güney,doğu,kuzey`) | Arşiv ülke dosyası | Yıllar |
|--------|-------|--------------------------------|--------------------|--------|
| `turkey` | Antalya–Muğla Akdeniz kıyısı (2021 Manavgat, Marmaris, Milas yangınları) | `27,36,32.5,37.5` | `Turkey` | 2020–2025 |
| `california` | Los Angeles havzası ve San Gabriel Dağları (Eylül 2020 Bobcat, Eylül 2024 Bridge, Ocak 2025 Palisades ve Eaton yangınları) | `-119,33.5,-117.5,34.5` | `United_States` | 2020–2025 |
| `australia` | Sidney havzası, Blue Mountains, Central Coast (Black Summer yangınlarının Ocak 2020 dönemi: Gospers Mountain, Green Wattle Creek) | `149,-34.5,151,-33` | `Australia` | 2020–2025 |

Arşivdeki ABD dosyasının adı `United_States`'tir (`United_States_of_America` değil). Sınırlar kapsayıcıdır (`>=` ve `<=`).

## Satır sayıları

Sütunlar: kesilen ham satır sayısı (CSV dosyalarındaki satır = harmonize hattına giren satır) ve harmonize kayıt sayısı. VIIRS, S-NPP (`satellite = N`) ile NOAA-20 (`N20`) toplamıdır. NOAA-21 FIRMS ülke arşivinde yoktur.

### turkey

| Yıl | MODIS | VIIRS S-NPP | VIIRS NOAA-20 | Harmonize |
|-----|------:|------------:|--------------:|----------:|
| 2020 | 109 | 406 | 394 | 686 |
| 2021 | 2 748 | 7 807 | 8 610 | 8 657 |
| 2022 | 181 | 593 | 651 | 883 |
| 2023 | 117 | 364 | 404 | 668 |
| 2024 | 186 | 559 | 506 | 898 |
| 2025 | 232 | 624 | 647 | 980 |
| **Toplam** | **3 573** | **10 353** | **11 212** | **12 772** |

### california

| Yıl | MODIS | VIIRS S-NPP | VIIRS NOAA-20 | Harmonize |
|-----|------:|------------:|--------------:|----------:|
| 2020 | 1 832 | 7 443 | 6 828 | 6 905 |
| 2021 | 42 | 323 | 343 | 477 |
| 2022 | 27 | 342 | 481 | 579 |
| 2023 | 25 | 311 | 308 | 475 |
| 2024 | 663 | 2 933 | 3 494 | 3 648 |
| 2025 | 379 | 1 983 | 1 998 | 2 325 |
| **Toplam** | **2 968** | **13 335** | **13 452** | **14 409** |

### australia

| Yıl | MODIS | VIIRS S-NPP | VIIRS NOAA-20 | Harmonize |
|-----|------:|------------:|--------------:|----------:|
| 2020 | 970 | 4 949 | 5 311 | 5 615 |
| 2021 | 289 | 1 302 | 1 593 | 1 828 |
| 2022 | 77 | 402 | 463 | 522 |
| 2023 | 271 | 1 527 | 1 541 | 1 714 |
| 2024 | 204 | 1 020 | 1 049 | 1 346 |
| 2025 | 208 | 1 287 | 1 349 | 1 769 |
| **Toplam** | **2 019** | **10 487** | **11 306** | **12 794** |

## Dosya boyutları

| Klasör | `modis.csv` | `viirs.csv` | `harmonized.json` | Toplam |
|--------|------------:|------------:|------------------:|-------:|
| `turkey` | 282 814 B | 1 709 716 B | 3 614 376 B | 5 606 906 B |
| `california` | 243 066 B | 2 178 732 B | 4 114 381 B | 6 536 179 B |
| `australia` | 162 695 B | 1 773 908 B | 3 619 775 B | 5 556 378 B |
| **Toplam** | | | | **17 699 463 B (17,7 MB)** |

Yeni dosyalar için bütçe 25 MB'tır; sıkıştırma kullanılmadı, tüm dosyalar düz metindir.

## Dosya biçimleri

### `modis.csv`, `viirs.csv`

- Başlık satırı arşivdeki özgün FIRMS başlığıdır, sütunlar değiştirilmedi.
  - MODIS: `latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight,type`
  - VIIRS: `latitude,longitude,bright_ti4,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_ti5,frp,daynight,type`
- Satırlar arşivdeki metinle birebir aynıdır (yeniden biçimlendirme yok), `(acq_date, acq_time)` sırasına dizilidir. `viirs.csv` S-NPP ve NOAA-20 satırlarını birleştirir.
- Yalnızca `type = 0` (bitki örtüsü yangını) satırları vardır. Pipeline `type != 0` satırlarını zaten eler; böylece CSV satır sayısı `harmonized.json` içindeki `raw.modis_count` / `raw.viirs_count` ile birebir eşleşir. Elenen satır sayısı `meta.non_wildfire_dropped` alanındadır.
- Zaman UTC'dir. MODIS `confidence` 0–100 sayıdır, VIIRS `confidence` `l`/`n`/`h` harfidir.

### `harmonized.json`

`GET /api/harmonize` yanıtıyla aynı üst düzey yapı (`docs/API.md`, `web/src/services/api.ts` içindeki `WireHarmonizeResponse` ile uyumlu), tek satırlık (minified) JSON:

| Alan | İçerik |
|------|--------|
| `raw` | `modis_count`, `viirs_count`, `total_raw` (tip filtresinden sonra) |
| `harmonized` | `count`, `static_masked`, `sama_removed`, `hotspots[]` (`HarmonizedHotspot`) |
| `calendar` | Aylık `CalendarCell` dizisi (6 yıl × 12 ay = 72 hücre) |
| `effort` | `reliable`, `reason`, `factors[]`, `tropical_swath_gap` |
| `filters` | `min_confidence: "low"`, `excluded_detections: 0` |
| `warnings` | Boş dizi |
| `report` | `per_source` (`[arşiv kaynağı, bbox içindeki satır sayısı, tip filtresinden önce]`), `failures: []`, `transactions_spent: 0`, `uncovered: []` |

Gerçek API yanıtından farkları:

- `raw.hotspots` yoktur (`include_raw=false` varsayılanı); ham noktalar CSV dosyalarındadır.
- `uncertainty` bloğu yoktur. Bu blok sunucudaki sabit metindir ve frontend tipinde isteğe bağlıdır. Nokta başına `frp_lower` / `frp_upper` vardır.
- `report.per_source` anahtarları FIRMS API kaynak adları değil arşiv klasörleridir (`modis`, `viirs-snpp`, `viirs-jpss1`).
- Ek `meta` bloğu vardır: `region`, `bbox`, `years`, `countries`, `source`, `config`, `wildfire_only`, `non_wildfire_dropped`, `pipeline` (`interscan_removed`, `fleet_merged`, `matched_pairs`).

## Üretim

- **Taban commit:** `31a48ae` (S4 çalışma ağacı). Üretici (`crates/firms-client/examples/sample_region.rs`) ve `csv_rows_in_bbox` yardımcısı (`crates/firms-client/src/archive.rs`) S4 commit'iyle eklenir; veriyi üreten kodun commit'i `git log -- crates/firms-client/examples/sample_region.rs` ile bulunur.
- **Yapılandırma:** `PipelineConfig::default()`: statik kaynak maskesi açık (bir H3 hücresi bir takvim yılında en az 10 ayda tespit alırsa elenir), SAMA filtresi kapalı, eşleştirme 2 km / 180 dk, güven filtresi `low` (hepsi tutulur). Eşikler `GET /api/config` ile okunur.
- **Harmonizasyon:** her bölge için 2020–2025 tüm satırlar tek parti olarak, `/api/harmonize` ile aynı sırayla (tip filtresi, statik maske, interscan, fleet, eşleştirme, VZA düzeltmesi). `calendar` ve `effort` aynı çekirdek fonksiyonlarla (`build_calendar_with`, `estimate_effort`, `calendar_swath_gap`) üretilir.
- **Arşiv:** `modis`, `viirs-snpp`, `viirs-jpss1` (NOAA-20) ülke-yıl dosyaları, salt okunur.

### Yeniden üretme

Arşiv dizini `ARCHIVE_CACHE_DIR` ile verilir (varsayılan `.cache/firms-archive`). Repo kökünden:

```bash
export ARCHIVE_CACHE_DIR=/yol/firms-archive

cargo run --release --locked -p pyrosync-firms --example sample_region -- \
  --out data/sample/turkey --name "Turkey: Antalya-Mugla Mediterranean coast" \
  --bbox 27,36,32.5,37.5 --from 2020 --to 2025 Turkey

cargo run --release --locked -p pyrosync-firms --example sample_region -- \
  --out data/sample/california --name "California: Los Angeles basin and San Gabriel Mountains" \
  --bbox -119,33.5,-117.5,34.5 --from 2020 --to 2025 United_States

cargo run --release --locked -p pyrosync-firms --example sample_region -- \
  --out data/sample/australia --name "Australia: Sydney basin, Blue Mountains and Central Coast" \
  --bbox 149,-34.5,151,-33 --from 2020 --to 2025 Australia
```

Samet'in makinesinde (Smart App Control) yerel cargo engelli olduğundan Docker kullanıldı: depo `/app:ro`, arşiv `/archive:ro`, çıktı için ayrıca yazılabilir bir dizin (`/out`) bağlanır ve `--out /out/turkey` verilir; sonra dosyalar `data/sample/` altına kopyalanır.

Aynı araç S7 (`data/case-study/turkey/`) için başka bbox ve yıl aralığıyla yeniden kullanılabilir (`--out`, `--bbox`, `--from`, `--to` değişir). Bölgeye birden çok ülke dosyası gerekiyorsa ülke adları art arda verilir.

## Bilinçli sınırlamalar

- **Alt bölgeler.** Tam kapsam bütçeye sığmaz (2020–2025, `type 0` satır sayıları, yaklaşık): Türkiye tamamı (26–45°D, 36–42°K) 392 bin satır, Kaliforniya eyaleti 1,27 milyon satır, Avustralya tamamı 14,3 milyon satır. Bu setteki ~225 bayt/ham satır oranıyla tam sürüm Türkiye için ~90 MB, Kaliforniya için ~290 MB, Avustralya için ~3 GB tutar. Yıl aralığı (2020–2025) ve yıl boyu tüm aylar korundu; küçültme yalnızca mekânsal oldu. Satır rastgele örneklenmedi, mevsim penceresi kullanılmadı (statik kaynak maskesi bir yılda en az 10 aylık veri ister, kısa pencerede çalışmaz).
- **Bölge kenarı etkisi.** Harmonizasyon yalnızca bbox içindeki satırlarla yapılır. Kenara yakın bir yangının bbox dışındaki bölümü bulunmaz ve kenardaki eşleştirme/statik maske kararları tüm ülke koşusundan farklı çıkabilir.
- **Küçük bbox ve gözlem çabası.** Efor çarpanı `effort_omega` her ayın aktif uydu yapılandırmasını o aydaki tespitlerden çıkarır. Küçük bir bölgede MODIS tespiti olmayan ayda yapılandırma "yalnızca VIIRS" sayılır, Ω `null` olur ve `count_normalized` ile `anomaly` o ay için `null` kalır (72 hücrenin Türkiye'de 4'ünde, Kaliforniya'da 23'ünde, Avustralya'da 10'unda). `anomaly` ayrıca aynı takvim ayında en az 3 hücre ister; 6 yıllık seride z-skoru yalnızca 6 değerden hesaplanır.
- **Yalnızca bitki örtüsü satırları.** `type != 0` satırları (Kaliforniya VIIRS'te 33 348, Avustralya VIIRS'te 28 261 satır) CSV'ye alınmadı; sayıları `meta.non_wildfire_dropped` içindedir.
- **`uncertainty` bloğu yok** (yukarıya bakınız).
- **NOAA-21 yok.** FIRMS ülke arşivinde bulunmadığından VIIRS yalnızca S-NPP ve NOAA-20'dir.
- **Güney yarımküre.** Avustralya'nın yangın mevsimi Aralık–Şubat'tır; takvim hücreleri takvim yılına göre gruplanır, Aralık 2019 serinin dışındadır.
