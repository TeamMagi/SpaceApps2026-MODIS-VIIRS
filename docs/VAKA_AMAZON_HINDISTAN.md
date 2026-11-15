# Vaka Çalışmaları — Amazon 2019–2021 ve Hindistan Şehir Duman Maruziyeti (Y8)

> **Görev:** Y8 · **Veri:** yerel FIRMS ülke arşivi (`.cache/firms-archive`, SP, `type 0`) · **Pipeline:** `PipelineConfig::default()`
> **Üretici:** `crates/firms-client/examples/amazon_india_case.rs` · **Çıktılar:** `docs/vaka/amazon_*`, `docs/vaka/india_*`
> **(Ölçüm)** = kendi arşivimizden hesaplanan değer; **(Literatür)** = makaleden aktarılan değer.

## 1. Amazon 2019–2021 — DURUM: ölçülmedi

**Tasarım:** Brezilya dosyaları, Legal Amazon sınır kutusu `[-74, -18, -44, 5.3]` ve bunun içindeki orman-biyom hücreleri (`crates/core/src/biome.rs`, `Forest`), 2012–2021. 2012–2018 ortalaması taban; ham toplam sayım, ham MODIS sayımı, ham FRP, harmonize `count_normalized` ve `total_frp` için 2019–2021 anomalisi (yüzde ve taban standart sapmasına göre z). Ω dönemleri C11 ile aynı (`2012-01..2018-03`, `2018-04..2021-12`). Manşet metrik `count_normalized`; `total_frp` C11 §3.5'teki Ω uyarısıyla birlikte verilir (Ω FRP'ye uygulanmaz, NOAA-20 girişinin FRP etkisi harmonizasyondan sonra da kısmen kalır).

**Yağış:** bu çalışmada yağış verisi **indirilmedi ve ölçülmedi**. 2019–2021'in normal yağışlı yıllar olduğu iddiası yalnız `[20, Şek. 10–11]`'den (Rizza et al. 2026, hakemsiz ön baskı) aktarılır; `[20]`'nin `VIIRS=high` filtresi burada kullanılmaz, kendi tam arşivimiz kullanılır.

**Sonuç sayıları:** program bu belge yazılırken Brezilya için takvim aşamasında (Ω havuzlama, 2012–2018 dönemi) çalışıyordu; ~10 dakikada tamamlanmadığı için **Amazon sayıları bu belgeye girmedi (ölçülmedi)**. Üretim sonrası `docs/vaka/amazon_summary.json`, `amazon_annual.csv`, `amazon_monthly.csv`, `amazon_series.svg`, `amazon_anomaly.svg` oluşur ve sayılar buraya aktarılmalıdır. Not: Brezilya VIIRS yılı ~116 MB; yıl başına yükleme+harmonizasyon 12–33 sn, takvim kurulumu (iki bölge) dakikalar sürer.

## 2. Hindistan — şehir kutularında duman maruziyeti (Ölçüm)

### 2.1 Yöntem
- **Şehirler:** GeoNames `cities15000` (public, `download.geonames.org`, `.cache/geonames/`, repoya eklenmez); `country=IN`, `nüfus > 100.000` → **538 şehir**. `[21]`'in `275` şehri ile eşleşmez (GeoNames'in nüfus alanı kent/ilçe tanımı farklıdır); 275'lik liste elimizde yok.
- **Kutu:** şehir merkezi etrafında 20×20 km (`±10 km`, boylam `cos(enlem)` ile düzeltilmiş).
- **Yıllar:** 2019–2023 (MODIS + S-NPP + NOAA-20; NOAA-21 arşivde yok). Hindistan ülkesi her yıl bütün olarak harmonize edilir, sonra kutulara atanır (kutu kesişen şehirlerde bir tespit her şehre sayılır; "birleşim" satırı bir kez sayar).
- **Harmonize oran:** `(matched + viirs_only) / (matched + modis_only)`.

### 2.2 Sonuçlar (2019–2023 toplam, birleşim)
| Ölçüt | Şehir kutuları | Hindistan geneli |
|---|---|---|
| Ham MODIS / S-NPP / NOAA-20 | 19.568 / 145.735 / 148.596 | 399.988 / 2.632.784 / 2.646.568 |
| Harmonize tespit | 222.333 | 3.898.507 |
| Ham S-NPP / MODIS | **7,45×** | 6,58× |
| Ham VIIRS (S-NPP+NOAA-20) / MODIS | 15,04× | 13,20× |
| Harmonize VIIRS / MODIS | 11,82× | 10,26× |
| `<1 MW` payı: ham MODIS / ham VIIRS / harmonize | 0,0% / 11,0% / 10,3% | 0,0% / 7,8% / 7,8% |
| FRP (GW): ham MODIS / ham VIIRS / harmonize | 358 / 1.543 / 1.244 | 11.403 / 36.189 / 27.713 |

- Yıllık S-NPP/MODIS oranı şehir kutularında 6,99–7,15× (2019–2021) aralığındadır; **`[21]`'in 6,3–7,7× aralığı tek-uydu S-NPP/MODIS oranıyla uyumludur** (Hindistan geneli 6,58×). İki VIIRS uydusu toplanınca ham oran 14–16,7×'e çıkar: bu uydu eklemenin yapay etkisidir, gerçek yangın farkı değil.
- Harmonizasyon aynı yangının birden çok uydu tarafından sayılmasını giderir: kutularda ham toplam 313.899 → harmonize 222.333 (−%29). Şehir başına ham VIIRS/MODIS oranı (MODIS ≥ 20 tespitli 197 şehir): medyan 14,3×, p10–p90 8,0–26,3×.
- En yoğun kutular Pencap şehirleridir (ör. Barnala, Moga, Muktsar, Firozpur: kutu başına ~5.000 harmonize tespit); `india_cities.csv`.
- Ham MODIS'te `<1 MW` payı %0 çıkar: MODIS FRP'si bu eşiğin altını neredeyse hiç vermez; küçük yangınlar yalnız VIIRS'te görünür (%11).

### 2.3 Sınırlar
- Harmonize oran tanımı eşleşmeyen VIIRS tespitlerini (`viirs_only`, kutularda 204.217) yapısal olarak büyütür; `[21]`'in ham VIIRS/MODIS tanımıyla doğrudan karşılaştırma için ham S-NPP/MODIS satırı kullanılmalıdır.
- `[21]`'in tam yöntemi (275 şehir, on yıllık seri, Fire-Year) yeniden üretilmedi; duman/PM2.5 maruziyeti ölçülmedi, yalnız yangın sayısı/FRP.
- NOAA-21 yok; 2019–2023 aralığı `[21]`'in on yıllık aralığından kısadır.

## 3. Yeniden üretim
```bash
cargo run --release -p pyrosync-firms --example amazon_india_case -- india    # ~1 dk
cargo run --release -p pyrosync-firms --example amazon_india_case -- amazon   # uzun, bkz. §1
```
Şehir dosyası: `.cache/geonames/cities15000.txt`. Çıktılar: `vaka/india_annual.csv`, `india_cities.csv`, `india_summary.json`, `india_ratio.svg`, `india_cities.svg`; Amazon için `vaka/amazon_*`.
