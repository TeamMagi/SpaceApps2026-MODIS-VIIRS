# PyroSync API Referansı

Bu doküman PyroSync sunucusunun HTTP API'sini tanımlar. Frontend ve backend geliştiricileri bu sözleşmeye göre çalışır. Sunucu Rust (Axum) ile yazılmıştır; FIRMS canlı verisi, yerel FIRMS ülke arşivi ve türetilmiş (önceden hesaplanmış) seri önbelleği olmak üzere üç kaynağı kullanır.

Son doğrulama: 2026-11-15 · commit 8846819 · örnekler bu sürümden yakalandı.

## Genel bilgiler

- **Base URL:** `http://localhost:3001/api`. Port `PORT` ortam değişkeninden okunur, varsayılan `3001`dir. Geliştirme ortamında Vite dev sunucusu (`web/vite.config.ts`) `/api` isteklerini `http://localhost:3001` adresine yönlendirir. Docker Compose aynı portu yayınlar.
- Her endpoint `GET` ile çağrılır ve JSON (UTF-8) yanıt döner. `export` endpoint'i dosya (CSV veya GeoJSON) döner. Alan adları snake_case'tir. Enum değerleri PascalCase string'lerdir.
- Tarihler `YYYY-MM-DD` biçimindedir. Zaman damgaları UTC üzerinde RFC 3339 biçimindedir (örnek: `2021-08-01T00:52:00Z`).
- İstemci kimlik doğrulaması yoktur. `FIRMS_API_KEY` yalnızca sunucuda tutulur ve hiçbir yanıtta görünmez. Hata mesajlarında anahtar `***` olarak gösterilir.
- `FIRMS_API_KEY` tanımlı değilse FIRMS endpoint'leri (`availability`, `harmonize`, `compare`, `stats`, `export`) `503` döner. Arşiv endpoint'leri (`calendar`, `regions`, `series`, `points`) anahtar gerektirmez.
- Hata gövdesi her zaman `{"error": "..."}` biçimindedir. Sorgu parametresi eksik veya hatalı olduğunda da JSON 400 döner.
- CORS: `CORS_ALLOWED_ORIGINS` tanımlıysa yalnızca listedeki origin'ler `GET` metodunu ve `content-type` başlığını kullanabilir. Tanımlı değilse CORS izni geneldir (permissive).
- `X-Cache: HIT|MISS` başlığı `harmonize`, `compare`, `stats`, `export` ve `calendar` yanıtlarında bulunur.
  - `days` ile yapılan istekler 600 sn önbellekte kalır.
  - `start`/`end` ile yapılan istekler ve `calendar` 21600 sn (6 saat) önbellekte kalır.
  - Önbellek anahtarı rota, bbox, zaman penceresi ve `include_raw` değerini kapsar. `export` için rota `export-csv` veya `export-geojson` olarak ayrılır.
  - Hatalar önbelleğe alınmaz.
  - `CACHE_BUDGET_MB` varsayılanı 128'dir. `0` önbelleği kapatır. Bütçe dolduğunda en eski girdiler önce çıkarılır.
- `/api/` altındaki bilinmeyen yollar JSON 404 döner: `Unknown API endpoint: /api/...`. `/api` dışındaki yollar `STATIC_DIR` (varsayılan `web/dist`) üzerinden servis edilir. Dosya yoksa ve `index.html` varsa SPA için `index.html` döner.

## Endpoint özeti

| Yol | Döndürdüğü şey | Veri kaynağı | FIRMS anahtarı | X-Cache |
|-----|----------------|--------------|----------------|---------|
| `GET /api/health` | Sunucu durumu ve sürüm | Yok | Gerekmez | Yok |
| `GET /api/config` | Etkin `PipelineConfig` | Yapılandırma (varsayılan, TOML, env) | Gerekmez | Yok |
| `GET /api/availability` | FIRMS kaynaklarının tarih aralıkları | FIRMS canlı API | Gerekir | Yok |
| `GET /api/harmonize` | Harmonize kayıtlar, takvim, effort, belirsizlik, rapor | FIRMS canlı API (NRT ve SP) | Gerekir | Var |
| `GET /api/compare` | Ham ve harmonize sayıların karşılaştırması | FIRMS canlı API (NRT ve SP) | Gerekir | Var |
| `GET /api/stats` | Toplamlar, sensör kırılımı, pipeline metrikleri | FIRMS canlı API (NRT ve SP) | Gerekir | Var |
| `GET /api/export` | Harmonize hotspot'lar, CSV veya GeoJSON dosyası | FIRMS canlı API (NRT ve SP) | Gerekir | Var |
| `GET /api/calendar` | Ülke bazında tarihsel aylık yanma takvimi | Yerel FIRMS ülke arşivi (yalnızca okuma) | Gerekmez | Var |
| `GET /api/regions` | Arşivdeki ülkeler, yıl aralıkları, boyut | Yalnızca yerel arşiv dizini | Gerekmez | Yok |
| `GET /api/series` | Bir bölgenin yıllık ve aylık serisi | Yerel arşiv; türetilmiş seri önbelleği | Gerekmez | Yok (`X-Series-Source` başlığı var) |
| `GET /api/points` | Bir bölge ve yılın (opsiyonel ay) nokta verisi | Yalnızca yerel arşiv dizini | Gerekmez | Yok |

`calendar`, `regions`, `series` ve `points` hiçbir zaman arşiv dışına indirme yapmaz. Eksik arşiv dosyaları `make data` ile hazırlanır.

## Ortak kavramlar

### Bounding box

`west`, `south`, `east`, `north` alanları derece cinsindendir.

- `west` ve `east` `[-180, 180]` aralığında olmalıdır.
- `south` ve `north` `[-90, 90]` aralığında olmalıdır.
- `west <= east` ve `south <= north` olmalıdır.
- Antimeridyeni geçen kutular desteklenmez.
- 90 ve -180 gibi sınır değerleri kabul edilir.

### Zaman penceresi (harmonize, compare, stats, export)

Zaman penceresi iki biçimden birinde verilir:

- `days`: 1 ile 5 arasında tam sayı. Bugün (UTC) dahil son N gündür. Hiçbiri verilmezse varsayılan `1`dir.
- `start` ve `end`: `YYYY-MM-DD` biçiminde, `end >= start` olmalıdır.

`days` ile `start` veya `end` birlikte gönderilirse 400 döner.

Sunucu pencereyi kaynak başına en fazla 5 günlük FIRMS isteklerine böler. Her gün bir FIRMS işlemi (transaction) harcar. FIRMS limiti 10 dakikada 5000 işlemdir. Her tarih için SP (bilimsel arşiv) kaynağı varsa o tercih edilir, yoksa NRT kullanılır. Hiçbir kaynağın kapsamadığı tarihler `report.uncovered` içinde ve `warnings` dizisinde raporlanır. FIRMS Area API pencereleri `AREA_CACHE_DIR` altında diske de önbelleğe alınır; önbellek isabeti FIRMS işlemi harcamaz. SP pencereleri süresiz, NRT pencereleri `AREA_CACHE_NRT_TTL_SECS` süresi kadar geçerlidir.

### Enum değerleri

| Enum | Değerler |
|------|----------|
| Sensor | `ModisTerra`, `ModisAqua`, `ViirsSNPP`, `ViirsNOAA20`, `ViirsNOAA21` |
| HotspotSource | `ModisOnly`, `ViirsOnly`, `Matched` |
| Confidence | `Low`, `Nominal`, `High` |
| DayNight | `Day`, `Night` |

`/api/points` ham satırlarında (`raw`) `sensor` alanı bu enum yerine `"MODIS"` veya `"VIIRS"` string'idir. `/api/export` CSV ve GeoJSON çıktısında enum değerleri yine PascalCase'dir.

### Harmonize edilmiş kayıt

`harmonized.hotspots[]`, `/api/points` içindeki `harmonized[]` (ek olarak `id` alanıyla) ve `/api/export` kayıtları aynı alanları taşır (`HarmonizedHotspot`).

| Alan | Tip | Açıklama |
|------|-----|----------|
| `lat` | number | Enlem (derece) |
| `lon` | number | Boylam (derece) |
| `frp` | number | Birleştirilmiş kayıtta kaynak FRP değeri (MW) |
| `frp_corrected` | number | VZA ve MODIS ölçek düzeltmesi uygulanmış FRP (MW) |
| `frp_lower` | number | `frp_corrected` için analitik güven aralığının alt sınırı (MW) |
| `frp_upper` | number | `frp_corrected` için analitik güven aralığının üst sınırı (MW) |
| `vza_factor` | number \| null | MODIS kaynaklı düzeltme faktörü. Düzeltme uygulanmadıysa `null` |
| `n_obs` | integer | Bu kayda birleşen gözlem sayısı |
| `platforms` | array of Sensor | Gözlemi veren uydular |
| `confidence` | Confidence | Güven seviyesi |
| `source` | HotspotSource | `ModisOnly`, `ViirsOnly` veya `Matched` |
| `acquired` | RFC 3339 zaman | Gözlem zamanı (UTC) |
| `daynight` | DayNight | Gündüz veya gece |

`frp_lower` ve `frp_upper` model tahminidir. Bkz. "Belirsizlik".

### Harmonizasyon hattı

Ham FRP kayıtlarından önce `type` değeri 0 olmayanlar (yanardağ, statik kaynak, deniz üstü) çıkarılır. Ardından aynı sensörün tarama (interscan) çakışmaları kaldırılır. Aynı uydu ailesine ait kardeş uydular (fleet) birleştirilir: MODIS için 180 dakika, VIIRS için 60 dakika penceresi kullanılır. Son olarak MODIS ve VIIRS kayıtları 2 km ve 180 dakika içinde eşleştirilir ve tek kayda birleştirilir (`Matched`). Eşleşmeyen kayıtlar her iki sensörden korunur. MODIS FRP değerleri görüş açısına (VZA) göre düzeltilir ve kalibrasyon bantlarıyla (`/api/config` altındaki `calibration`) işlenir. Tüm eşikler `GET /api/config` ile okunabilir.

### CalendarCell

`calendar` dizisindeki her öğe bir ay hücresidir. Yalnızca en az bir tespiti olan aylar listelenir; tespiti olmayan aylar hiç yer almaz.

| Alan | Tip | Açıklama |
|------|-----|----------|
| `year` | integer | Yıl |
| `month` | integer | Ay (1-12) |
| `count` | integer | O aydaki **harmonize kayıt** sayısı (ham tespit sayısı değildir; ham sayılar yanıtın `counts.raw_detections` alanındadır) |
| `count_normalized` | number \| null | `count / effort_omega`. Ω yoksa `null` |
| `effort_omega` | number \| null | Ay için gözlem çabası Ω (MODIS'e göre) |
| `mean_frp` | number | Yerel piksel ortalaması: `frp_corrected` ortalaması. MODIS'ten VIIRS'e geçişte süreksizdir |
| `mean_frp_raw` | number | `frp` ortalaması |
| `mean_frp_lower`, `mean_frp_upper` | number \| null | `mean_frp` için analitik güven sınırları |
| `total_frp` | number | Aylık toplam `frp_corrected` (MW). VIIRS tespiti olmayan hücrelerde (kalibrasyon açıksa) `k_c(lat)` ile çarpılır |
| `total_frp_lower`, `total_frp_upper` | number \| null | `total_frp` için analitik güven sınırları |
| `frp_per_detection_eq` | number \| null | MODIS eşdeğeri FRP / normalize tespit (`total_frp / count_normalized`). 2012 MODIS→VIIRS geçişinde süreklilik için `mean_frp` yerine önerilen metrik |
| `anomaly` | number \| null | `count_normalized` değerinin aynı takvim ayındaki diğer yıllara göre z-skoru |
| `sfai` | number \| null | `count_normalized` için dayanıklı z-skoru (SFAI): `(x − medyan) / (0.65 × 1.4826 × MAD + 0.35 × σ)`; σ popülasyon standart sapmasıdır. Payda ağırlıkları flare-chronos referans projesinden gelir, medyan payı bize aittir. `anomaly` ile aynı havuz (aynı takvim ayı); en az 3 değer gerekir, payda 0 ise `null` |

`anomaly` hesabı: aynı takvim ayında en az 3 hücre ve sıfırdan farklı yayılım (standart sapma) gerekir. Standart sapma popülasyon standart sapmasıdır. Bu koşullar sağlanmazsa `null` döner.

`seasonality` (yalnızca `/api/calendar`): metrik harmonize aylık `count`'tur (`"metric": "count"`). Her mevsim yılı için Zhang & Qian (2003) yoğunlaşma derecesi/dönemi hesaplanır: 12 aylık değer `r_i` için açı `θ_i = (takvim_ayı − 0.5) × 30°` (ay aralığının orta noktası, Cui et al. 2021 Denk. 1–2; Ocak = 15°, Yangın Yılı modunda da takvim ayına sabit), `pcd = √(Rx² + Ry²) / R` ([0, 1]; 1 = tüm yangınlar tek ayda), `pcp_deg = atan2(Rx, Ry)` ([0, 360)), `peak_month = pcp_deg / 30 + 0.5` ((0.5, 12.5] aralığında kesirli takvim ayı; tek aylık girdi tam o ayı verir, Aralık+Ocak 12.5). Makale 36 on günlük aralık kullanır ve PCP'yi on günlük birimle verir; biz 12 aylık aralık kullandığımız için değerler doğrudan karşılaştırılamaz. `calendar_year` Ocak–Aralık'ı, `fire_year` Mart Y – Şubat Y+1 dönemini Y etiketiyle verir. Kapsam içinde olup hücresi olmayan ay 0 sayılır; 12 ayı tam kapsamda olmayan yıl `complete: false` ile döner ve `pcd`/`pcp_deg`/`peak_month` `null` olur (`total` yine kapsamdaki aylardan hesaplanır). Toplam sıfırsa üçü de `null`; ay dağılımı tamamen düzgünse `pcd ≈ 0` ve yön tanımsız olduğundan `pcp_deg`/`peak_month` `null` olur. Kapsam, istenen yıl aralığının arşiv kapsamıyla (MODIS Terra'nın ilk ayı 2000-11) kesişimidir.

Terra'nın tek başına çalıştığı dönem (2002-07'den önce) için: seri 2002-07 veya sonrasını da içeriyorsa bu aylar karşılaştırılamaz sayılır. Bu aylarda `count_normalized`, `effort_omega` ve `anomaly` `null` olur; `count` ve FRP alanları ham değerleriyle kalır. Bu durumda `warnings` içinde bir uyarı bulunur. Seri tamamen 2002-07 öncesindeyse bu dışlama uygulanmaz.

### Belirsizlik

Yanıtlarda üst düzey bir `uncertainty` nesnesi bulunur (`harmonize` ve `calendar`):

| Alan | Tip | Açıklama |
|------|-----|----------|
| `confidence` | number | `z` çarpanına karşılık gelen iki kuyruklu güven düzeyi (`z = 1.0` için ~0.6827) |
| `note` | string | Model varsayımlarını ve sınırlılıklarını açıklayan sabit metin |

Güven aralıkları (`frp_lower`, `frp_upper`, `mean_frp_*`, `total_frp_*`) rastgele bileşeni (sensör gürültüsü ve VZA hatası, `1/√n` ölçeğiyle) ve sistematik bileşeni (MODIS ölçek belirsizliği ve `k_c` belirsizliği) birleştirerek hesaplanır. Varsayılan parametreler `uncertainty` altında `/api/config` ile okunabilir. Bu değerler model varsayımlarıdır, yayımlanmış ölçümler değildir.

### warnings

String dizisidir. Boş olabilir. Şu durumlarda uyarı eklenir:

- Bir FIRMS isteği başarısız olduğunda (bkz. `effort.reliable`).
- İstenen zaman aralığının bir kısmı hiçbir kaynak tarafından kapsanmadığında.
- Arşiv yıl aralığı kırpıldığında (`calendar`).
- Takvimde NOAA-21 eksikliği bildirildiğinde (`calendar`).
- Yıl aralığında Terra-only dönem dışlandığında (bkz. "CalendarCell").
- Yerel arşivde beklenen dosya yoksa (`calendar`).

### report

| Alan | Tip | Açıklama |
|------|-----|----------|
| `per_source` | array of [string, integer] | Kaynak adı ve o kaynaktan gelen kayıt sayısı çiftleri |
| `failures` | array of [string, string] | Başarısız kaynak ve hata mesajı çiftleri |
| `transactions_spent` | integer | Harcanan FIRMS işlem sayısı |
| `uncovered` | array of [string, string] | Hiçbir kaynağın kapsamadığı `[başlangıç, bitiş]` tarih çiftleri |

### effort ve effort.reliable

`effort` alanı Ω faktörlerini içerir. `reliable` değeri yalnızca şu durumda `false` olur: bir FIRMS isteği (harmonize, compare, stats, export için) veya bir ülke-yıl arşiv dosyası (`calendar` için) başarısız olduğunda. Bu durumda `reason` açıklama içerir ve `warnings` aynı bilgiyi taşır. Diğer durumlarda `reliable` `true` ve `reason` `null` olur.

| Alan | Tip | Açıklama |
|------|-----|----------|
| `reliable` | boolean | Yukarıdaki koşul geçerliyse `false` |
| `reason` | string \| null | Güvenilmezlik nedeni. Yoksa `null` |
| `factors` | array | Uydu yapılandırmasına göre Ω faktörleri (aşağıda) |

`factors[]` öğeleri:

| Alan | Tip | Açıklama |
|------|-----|----------|
| `has_modis` | boolean | Yapılandırmada MODIS var mı |
| `viirs_platforms` | array of Sensor | Yapılandırmadaki VIIRS uyduları |
| `platforms` | array of Sensor | Yapılandırmadaki tüm uydular |
| `months` | integer | Bu yapılandırmanın göründüğü ay sayısı |
| `total` | integer | Toplam harmonize kayıt sayısı |
| `modis_observable` | integer | MODIS ile gözlemlenebilen kayıt sayısı |
| `omega` | number \| null | Ω değeri. Ay sayısı azsa (kodda eşik 3 aydır) `null` olabilir |
| `total_high_intensity` | integer | `frp >= 40.0` MW (sonlu) toplam harmonize kayıt sayısı |
| `modis_observable_high_intensity` | integer | `frp >= 40.0` MW (sonlu) MODIS ile gözlemlenebilen kayıt sayısı |
| `omega_high_intensity` | number \| null | Yüksek şiddet (≥ 40 MW) rejimi için Ω; 3+ ay ve `modis_observable_high_intensity >= 30` gerekir |
| `snpp_aqua_high_intensity_ratio` | number \| null | Gündüz ≥ 40 MW S-NPP / Aqua oranı (diurnal/swath tanı metriği, ayrı raporlanır ve sayımlara uygulanmaz; S-NPP yoksa veya Aqua < 30 ise `null`) |

## GET /api/health

Sunucunun ayakta olduğunu ve sürümünü döndürür. Her zaman `200` döner. FIRMS anahtarı gerekmez.

### Parametreler

Parametre yok.

### Örnek istek

```bash
curl -s "http://localhost:3001/api/health"
```

### Örnek yanıt

```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

### Alanlar

| Alan | Tip | Açıklama |
|------|-----|----------|
| `status` | string | Her zaman `"ok"` |
| `version` | string | Sunucu paketinin sürümü |

### Hatalar

Bu endpoint hata döndürmez.

### Notlar

- Frontend bu endpoint'i kullanmaz.

## GET /api/config

Sunucunun kullandığı etkin `PipelineConfig` değerini döndürür. Değerler şu sırayla oluşur: kodda tanımlı varsayılanlar, ardından TOML dosyası, ardından `PYROSYNC_*` ortam değişkenleri. Sonraki kaynak öncekini ezer.

### Parametreler

Parametre yok.

### Örnek istek

```bash
curl -s "http://localhost:3001/api/config"
```

### Örnek yanıt

```json
{
  "dedup": {
    "max_time_diff_secs": 8,
    "min_overlap_ratio": 0.3
  },
  "fleet": {
    "viirs_window_secs": 3600,
    "modis_window_secs": 10800,
    "radius_km": 2
  },
  "matching": {
    "radius_km": 2,
    "window_secs": 10800
  },
  "vza": {
    "a": 0.2017,
    "b": 0.0168,
    "c": 1
  },
  "calibration": {
    "enabled": true,
    "bands": [
      {
        "min_abs_lat": 0,
        "max_abs_lat": 30,
        "ratio": 1.11,
        "ratio_sigma": 0.08
      },
      {
        "min_abs_lat": 30,
        "max_abs_lat": 50,
        "ratio": 1.18,
        "ratio_sigma": 0.15
      },
      {
        "min_abs_lat": 55,
        "max_abs_lat": 70,
        "ratio": 1.28,
        "ratio_sigma": 0.11
      }
    ]
  },
  "uncertainty": {
    "modis_sensor_sigma": 0.12,
    "viirs_sensor_sigma": 0.08,
    "vza_error_coeff": 0.05,
    "modis_scale_sigma": 0.1,
    "z": 1
  }
}
```

### Alanlar

| Alan | Tip | Açıklama |
|------|-----|----------|
| `dedup.max_time_diff_secs` | integer | Interscan duplikat zaman farkı eşiği (sn) |
| `dedup.min_overlap_ratio` | number | Interscan duplikat minimum örtüşme oranı (0.0-1.0) |
| `fleet.viirs_window_secs` | integer | VIIRS filo birleştirme penceresi (sn) |
| `fleet.modis_window_secs` | integer | MODIS filo birleştirme penceresi (sn) |
| `fleet.radius_km` | number | Filo birleştirme yarıçapı (km) |
| `matching.radius_km` | number | MODIS/VIIRS eşleştirme yarıçapı (km, en fazla 10) |
| `matching.window_secs` | integer | MODIS/VIIRS eşleştirme zaman penceresi (sn) |
| `vza.a`, `vza.b`, `vza.c` | number | MODIS VZA düzeltme polinomu `R_rel(θ) = a·θ² + b·θ + c` katsayıları (θ radyan) |
| `calibration.enabled` | boolean | Pre-VIIRS tamlık katsayısı `k_c` uygulansın mı |
| `calibration.bands[]` | array | Enlem bantları: `min_abs_lat`, `max_abs_lat` (|lat|, derece), `ratio` (`k_c`), `ratio_sigma` (`k_c` 1σ). 50-55° arası doğrusal geçiş, 70°'nin üstü son banda sabitlenir |
| `uncertainty.modis_sensor_sigma`, `uncertainty.viirs_sensor_sigma` | number | Tek piksel sensör gürültüsü (göreli 1σ) |
| `uncertainty.vza_error_coeff` | number | VZA düzeltmesi arttıkça eklenen göreli hata katsayısı |
| `uncertainty.modis_scale_sigma` | number | MODIS ölçek/Terra vekil belirsizliği (göreli 1σ) |
| `uncertainty.z` | number | Güven aralığı z çarpanı (`> 0`) |

### Hatalar

| Durum | Ne zaman | Gövde |
|-------|----------|-------|
| — | Bu endpoint hata döndürmez. Geçersiz yapılandırma ile sunucu zaten başlamaz. | — |

### Notlar

- Yapılandırma değişirse türetilmiş seri önbelleği geçersiz sayılır (bkz. `/api/series`).
- TOML dosyası ve ortam değişkenleri için "API'yi etkileyen ortam değişkenleri" bölümüne bakın.

## GET /api/availability

FIRMS'in MODIS, VIIRS S-NPP, NOAA-20 ve NOAA-21 için bildirdiği NRT ve SP kaynaklarının tarih aralıklarını listeler. Sonuç `source` alanına göre sıralanır.

### Parametreler

Parametre yok.

### Örnek istek

```bash
curl -s "http://localhost:3001/api/availability"
```

### Örnek yanıt

Bu örnek test düzeneğindeki sahte FIRMS verisinden alındı; sayılar sentetik, yapı gerçek.

```json
[
  {
    "source": "MODIS_NRT",
    "min_date": "2026-09-16",
    "max_date": "2026-11-17"
  },
  {
    "source": "VIIRS_NOAA20_NRT",
    "min_date": "2026-09-16",
    "max_date": "2026-11-17"
  },
  {
    "source": "VIIRS_NOAA21_NRT",
    "min_date": "2026-09-16",
    "max_date": "2026-11-17"
  },
  {
    "source": "VIIRS_SNPP_NRT",
    "min_date": "2026-09-16",
    "max_date": "2026-11-17"
  }
]
```

### Alanlar

Yanıt bir dizidir. Her öğe:

| Alan | Tip | Açıklama |
|------|-----|----------|
| `source` | string | FIRMS kaynak kimliği (ör. `MODIS_NRT`, `MODIS_SP`, `VIIRS_SNPP_NRT`, `VIIRS_SNPP_SP`, `VIIRS_NOAA20_NRT`, `VIIRS_NOAA20_SP`, `VIIRS_NOAA21_NRT`, `VIIRS_NOAA21_SP`) |
| `min_date` | string (`YYYY-MM-DD`) | Kaynağın ilk kullanılabilir günü |
| `max_date` | string (`YYYY-MM-DD`) | Kaynağın son kullanılabilir günü |

MODIS için yalnızca Terra sorgulanır. MODIS_NRT ve MODIS_SP kaynakları Terra ve Aqua'yı birlikte kapsar.

### Hatalar

| Durum | Ne zaman | Gövde |
|-------|----------|-------|
| 503 | `FIRMS_API_KEY` tanımlı değilse | `{"error": "FIRMS_API_KEY is not configured"}` |
| 502 | FIRMS availability tablosu alınamazsa | `{"error": "Failed to fetch FIRMS availability: ..."}` |

Örnek (503):

```json
{
  "error": "FIRMS_API_KEY is not configured"
}
```

### Notlar

- FIRMS availability tablosu süreç başına bir kez alınır ve bir `OnceCell` içinde tutulur. Sunucu yeniden başlatılana kadar yenilenmez. Bkz. "Bilinen sorunlar".
- Frontend bu endpoint'i şu anda kullanmaz.

## GET /api/harmonize

Verilen bbox ve zaman penceresi için FIRMS kayıtlarını çeker, harmonize eder ve aylık takvim, effort ve belirsizlik bilgisini üretir. Yanıt `X-Cache` başlığı içerir.

### Parametreler

| Ad | Tip | Zorunlu | Varsayılan | Kural |
|----|-----|---------|------------|-------|
| `west` | number | Evet | — | `[-180, 180]`, `west <= east` |
| `south` | number | Evet | — | `[-90, 90]`, `south <= north` |
| `east` | number | Evet | — | `[-180, 180]` |
| `north` | number | Evet | — | `[-90, 90]` |
| `days` | integer | Hayır | `1` (hiçbir zaman parametresi yoksa) | `1..=5`; `start`/`end` ile birlikte kullanılamaz |
| `start` | string (`YYYY-MM-DD`) | Hayır | — | `end` ile birlikte verilmeli; `end >= start` |
| `end` | string (`YYYY-MM-DD`) | Hayır | — | `start` ile birlikte verilmeli |
| `include_raw` | boolean | Hayır | `false` | `true` ise `raw.hotspots` dizisi eklenir |

### Örnek istek

```bash
curl -s "http://localhost:3001/api/harmonize?west=26&south=36&east=45&north=42&days=1&include_raw=true"
curl -s "http://localhost:3001/api/harmonize?west=26&south=36&east=45&north=42&start=2020-01-01&end=2020-01-03"
```

### Örnek yanıt

#### include_raw=true

Bu örnek test düzeneğindeki sahte FIRMS verisinden alındı; sayılar sentetik, yapı gerçek.

Not: `$.raw.hotspots` dizisindeki 5 öğeden ilk 4 tanesi gösteriliyor.

```json
{
  "calendar": [
    {
      "anomaly": null,
      "count": 3,
      "count_normalized": null,
      "effort_omega": null,
      "frp_per_detection_eq": null,
      "mean_frp": 23.333333333333332,
      "mean_frp_lower": 21.650742738116804,
      "mean_frp_raw": 23.333333333333332,
      "mean_frp_upper": 25.01592392854986,
      "month": 11,
      "total_frp": 70,
      "total_frp_lower": 64.95222821435041,
      "total_frp_upper": 75.04777178564959,
      "year": 2026
    }
  ],
  "effort": {
    "factors": [
      {
        "has_modis": true,
        "modis_observable": 2,
        "months": 1,
        "omega": null,
        "platforms": [
          "ModisTerra",
          "ModisAqua",
          "ViirsSNPP"
        ],
        "total": 3,
        "viirs_platforms": [
          "ViirsSNPP"
        ]
      }
    ],
    "reason": null,
    "reliable": true
  },
  "harmonized": {
    "count": 3,
    "hotspots": [
      {
        "acquired": "2026-11-15T10:30:00Z",
        "confidence": "Nominal",
        "daynight": "Day",
        "frp": 30,
        "frp_corrected": 30,
        "frp_lower": 25.313850194456005,
        "frp_upper": 34.68614980554399,
        "lat": 40,
        "lon": 33,
        "n_obs": 1,
        "platforms": [
          "ModisAqua"
        ],
        "source": "ModisOnly",
        "vza_factor": 1
      },
      {
        "acquired": "2026-11-15T11:00:00Z",
        "confidence": "High",
        "daynight": "Day",
        "frp": 30,
        "frp_corrected": 30,
        "frp_lower": 27.6,
        "frp_upper": 32.400000000000006,
        "lat": 39.001,
        "lon": 32.001,
        "n_obs": 2,
        "platforms": [
          "ModisTerra",
          "ViirsSNPP"
        ],
        "source": "Matched",
        "vza_factor": null
      },
      {
        "acquired": "2026-11-15T11:00:00Z",
        "confidence": "Low",
        "daynight": "Day",
        "frp": 10,
        "frp_corrected": 10,
        "frp_lower": 9.200000000000001,
        "frp_upper": 10.8,
        "lat": 41,
        "lon": 34,
        "n_obs": 1,
        "platforms": [
          "ViirsSNPP"
        ],
        "source": "ViirsOnly",
        "vza_factor": null
      }
    ]
  },
  "raw": {
    "hotspots": [
      {
        "acquired": "2026-11-15T10:00:00Z",
        "daynight": "Day",
        "frp": 50,
        "lat": 39,
        "lon": 32,
        "sensor": "ModisTerra"
      },
      {
        "acquired": "2026-11-15T10:30:00Z",
        "daynight": "Day",
        "frp": 30,
        "lat": 40,
        "lon": 33,
        "sensor": "ModisAqua"
      },
      {
        "acquired": "2026-11-15T11:00:00Z",
        "daynight": "Day",
        "frp": 40,
        "lat": 39.001,
        "lon": 32.001,
        "sensor": "ViirsSNPP"
      },
      {
        "acquired": "2026-11-15T11:00:00Z",
        "daynight": "Day",
        "frp": 20,
        "lat": 39.0011,
        "lon": 32.0011,
        "sensor": "ViirsSNPP"
      }
    ],
    "modis_count": 2,
    "total_raw": 5,
    "viirs_count": 3
  },
  "report": {
    "failures": [],
    "per_source": [
      [
        "MODIS_NRT",
        2
      ],
      [
        "VIIRS_NOAA20_NRT",
        0
      ],
      [
        "VIIRS_NOAA21_NRT",
        0
      ],
      [
        "VIIRS_SNPP_NRT",
        3
      ]
    ],
    "transactions_spent": 4,
    "uncovered": []
  },
  "uncertainty": {
    "confidence": 0.6826894723352727,
    "note": "Harmonized values are model estimates, not ground truth. For MODIS detections, VZA scales raw FRP to MODIS footprint equivalents; VIIRS-era calendar total_frp adds empirically measured pre-VIIRS completeness (k_c) when no VIIRS detections are present in a cell. mean_frp remains native-pixel and is discontinuous at sensor transitions. Default sensor-noise, VZA error, and MODIS scale uncertainty parameters are assumptions, not published values."
  },
  "warnings": []
}
```

Aynı istek tekrarlanınca gövde birebir aynı döner; yalnızca `X-Cache: HIT` olur.

#### Kapsanmayan tarih aralığı (200, boş sonuç)

Bu örnek test düzeneğindeki sahte FIRMS verisinden alındı; sayılar sentetik, yapı gerçek.

Hiçbir kaynağın kapsamadığı aralıkta istek hata vermez. Sonuç boş döner ve aralık `report.uncovered` ile `warnings` içinde bildirilir.

```json
{
  "calendar": [],
  "effort": {
    "factors": [],
    "reason": null,
    "reliable": true
  },
  "harmonized": {
    "count": 0,
    "hotspots": []
  },
  "raw": {
    "modis_count": 0,
    "total_raw": 0,
    "viirs_count": 0
  },
  "report": {
    "failures": [],
    "per_source": [],
    "transactions_spent": 0,
    "uncovered": [
      [
        "2020-01-01",
        "2020-01-03"
      ]
    ]
  },
  "uncertainty": {
    "confidence": 0.6826894723352727,
    "note": "Harmonized values are model estimates, not ground truth. For MODIS detections, VZA scales raw FRP to MODIS footprint equivalents; VIIRS-era calendar total_frp adds empirically measured pre-VIIRS completeness (k_c) when no VIIRS detections are present in a cell. mean_frp remains native-pixel and is discontinuous at sensor transitions. Default sensor-noise, VZA error, and MODIS scale uncertainty parameters are assumptions, not published values."
  },
  "warnings": [
    "Requested time range has intervals not covered by any FIRMS source: 2020-01-01..2020-01-03."
  ]
}
```

#### Bir kaynak başarısız oldu (200, kısmi hata; excerpt)

Bu örnek test düzeneğindeki sahte FIRMS verisinden alındı; sayılar sentetik, yapı gerçek.

Not: Bu yanıt bir excerpt'tir; yalnızca `effort.reliable`, `effort.reason`, `warnings` ve `report` alanları gösteriliyor. Tam yanıtta `calendar`, `harmonized`, `raw` ve `uncertainty` alanları da bulunur.

```json
{
  "effort": {
    "reliable": false,
    "reason": "1 FIRMS request failed; missing source data may undercount monthly platform configurations and produce inaccurate effort_omega values."
  },
  "warnings": [
    "1 FIRMS request failed; missing source data may undercount monthly platform configurations and produce inaccurate effort_omega values."
  ],
  "report": {
    "failures": [
      [
        "VIIRS_NOAA20_NRT",
        "2026-11-15 (1d): FIRMS returned non-CSV response (HTTP 200 OK) from http://127.0.0.1:38479/api/area/csv/***/VIIRS_NOAA20_NRT/26,36,45,42/1/2026-11-15: Invalid MAP_KEY."
      ]
    ],
    "per_source": [
      [
        "MODIS_NRT",
        2
      ],
      [
        "VIIRS_NOAA21_NRT",
        0
      ],
      [
        "VIIRS_SNPP_NRT",
        3
      ]
    ],
    "transactions_spent": 4,
    "uncovered": []
  }
}
```

### Alanlar

| Alan | Tip | Açıklama |
|------|-----|----------|
| `raw.modis_count` | integer | Tip filtresinden sonra MODIS ham kayıt sayısı |
| `raw.viirs_count` | integer | Tip filtresinden sonra VIIRS ham kayıt sayısı (S-NPP, NOAA-20 ve NOAA-21 toplamı) |
| `raw.total_raw` | integer | `modis_count + viirs_count` |
| `raw.hotspots` | array | Yalnızca `include_raw=true` ile döner. Öğeler: `lat`, `lon`, `frp`, `sensor` (Sensor), `acquired`, `daynight` |
| `harmonized.count` | integer | `hotspots` dizisinin uzunluğu |
| `harmonized.hotspots` | array | Bkz. "Harmonize edilmiş kayıt" |
| `calendar` | array of CalendarCell | Aylık takvim. Bkz. "CalendarCell" |
| `effort` | object | Bkz. "effort ve effort.reliable" |
| `uncertainty` | object | Bkz. "Belirsizlik" |
| `warnings` | array of string | Bkz. "warnings" |
| `report` | object | Bkz. "report" |

`raw.hotspots` yalnızca `include_raw=true` olduğunda yanıta eklenir. Varsayılan istekte alan hiç bulunmaz.

### Hatalar

| Durum | Ne zaman | Gövde |
|-------|----------|-------|
| 400 | Bbox geçersiz, `days` 1..=5 dışında, `start`/`end` eksik veya tarih biçimi yanlış, `days` ile `start`/`end` birlikte | JSON |
| 400 | Zorunlu parametre yok veya tip hatalı (ör. `south` yok, `days=abc`) | JSON: `Failed to deserialize query string: ...` |
| 503 | `FIRMS_API_KEY` tanımlı değilse | JSON |
| 502 | Tüm FIRMS istekleri başarısız olduysa (veya availability tablosu alınamadıysa) | JSON |
| 504 | `REQUEST_TIMEOUT_SECS` süresi dolduysa | JSON |

Bbox ve zaman penceresi doğrulaması FIRMS anahtarı kontrolünden önce yapılır. Bu nedenle geçersiz bir istek anahtar yokken de 400 döner.

Örnek (400, eksik parametre):

```json
{
  "error": "Failed to deserialize query string: missing field `south`"
}
```

Örnek (400, `days` aralık dışı):

```json
{
  "error": "Invalid 'days' value 6: must be in the range 1..=5"
}
```

Örnek (400, `days` ile `start` birlikte):

```json
{
  "error": "'days' cannot be combined with 'start' or 'end'"
}
```

Örnek (503, anahtar yok):

```json
{
  "error": "FIRMS_API_KEY is not configured"
}
```

Örnek (502, tüm kaynaklar başarısız). Mesajda host `https://firms.modaps.eosdis.nasa.gov` olarak gösterilmiştir ve ilk kaynaktan sonrası kısaltılmıştır:

```json
{
  "error": "Upstream FIRMS request failed: MODIS_NRT: 2026-11-15 (1d): FIRMS returned non-CSV response (HTTP 200 OK) from https://firms.modaps.eosdis.nasa.gov/api/area/csv/***/MODIS_NRT/26,36,45,42/1/2026-11-15: Invalid MAP_KEY.; ..."
}
```

Örnek (504). Varsayılan `REQUEST_TIMEOUT_SECS` değeri 300 sn olduğu için gerçek mesaj `after 300s` içerir:

```json
{
  "error": "Request timed out after 300s; long date ranges can require hundreds of FIRMS requests. Increase REQUEST_TIMEOUT_SECS or narrow the date range."
}
```

### Notlar

- Önbellek: `days` istekleri 600 sn, `start`/`end` istekleri 21600 sn önbelleğe alınır. Hatalar önbelleğe alınmaz.
- Her 5 günlük parça kaynak başına bir FIRMS isteği olarak sınırlanır ve her gün bir işlem harcar. Büyük aralıklar `REQUEST_TIMEOUT_SECS` süresini aşabilir.
- `warnings` ve `effort.reliable` değerleri her FIRMS hatası için güncellenir. Frontend bu alanları kullanıcıya göstermelidir.
- Frontend `fetchHarmonize` ile alır.

## GET /api/compare

Aynı bbox ve zaman penceresi için ham ve harmonize sayıların, FRP toplamlarının ve birleştirme istatistiklerinin özetini döndürür. Yanıt kayıt listesi içermez. Parametreler `harmonize` ile aynıdır. `include_raw` yanıtı değiştirmez, yalnızca önbellek anahtarını etkiler.

### Parametreler

`harmonize` ile aynıdır. Bkz. [harmonize parametreleri](#get-apiharmonize).

### Örnek istek

```bash
curl -s "http://localhost:3001/api/compare?west=26&south=36&east=45&north=42&days=1"
```

### Örnek yanıt

Bu örnek test düzeneğindeki sahte FIRMS verisinden alındı; sayılar sentetik, yapı gerçek.

```json
{
  "counts": {
    "raw_total": 5,
    "raw_modis": 2,
    "raw_viirs": 3,
    "harmonized_total": 3,
    "removed_total": 2,
    "removed_pct": 40,
    "by_source": {
      "modis_only": 1,
      "viirs_only": 1,
      "matched": 1
    }
  },
  "frp": {
    "raw_sum": 150,
    "harmonized_raw_sum": 70,
    "harmonized_corrected_sum": 70,
    "reduction_pct": 53.333333333333336
  },
  "merging": {
    "records_with_multiple_observations": 1,
    "observations_merged": 1,
    "max_observations_in_one_record": 2,
    "platform_combinations": [
      {
        "platforms": [
          "ModisTerra",
          "ViirsSNPP"
        ],
        "records": 1
      },
      {
        "platforms": [
          "ModisAqua"
        ],
        "records": 1
      },
      {
        "platforms": [
          "ViirsSNPP"
        ],
        "records": 1
      }
    ]
  },
  "vza": {
    "records_with_correction": 1,
    "mean_factor": 1,
    "max_factor": 1,
    "mean_frp_reduction_pct": 0,
    "mean_frp_adjustment_pct": 0
  },
  "warnings": [],
  "report": {
    "per_source": [
      [
        "MODIS_NRT",
        2
      ],
      [
        "VIIRS_NOAA20_NRT",
        0
      ],
      [
        "VIIRS_NOAA21_NRT",
        0
      ],
      [
        "VIIRS_SNPP_NRT",
        3
      ]
    ],
    "failures": [],
    "transactions_spent": 4,
    "uncovered": []
  }
}
```

### Alanlar

| Alan | Tip | Açıklama |
|------|-----|----------|
| `counts.raw_total`, `raw_modis`, `raw_viirs` | integer | Ham kayıt sayıları (tip filtresinden sonra) |
| `counts.harmonized_total` | integer | Harmonize kayıt sayısı |
| `counts.removed_total` | integer | `raw_total - harmonized_total` (sıfırın altına düşmez) |
| `counts.removed_pct` | number | Kaldırılan kayıtların ham toplama oranı (%) |
| `counts.by_source` | object | `modis_only`, `viirs_only`, `matched` harmonize kaynaklarına göre sayımlar |
| `frp.raw_sum` | number | Ham FRP toplamı |
| `frp.harmonized_raw_sum` | number | Harmonize kayıtların `frp` toplamı |
| `frp.harmonized_corrected_sum` | number | Harmonize kayıtların `frp_corrected` toplamı |
| `frp.reduction_pct` | number | `100 * (1 - harmonized_corrected_sum / raw_sum)`. `raw_sum` sıfırsa `0` |
| `merging.records_with_multiple_observations` | integer | `n_obs > 1` olan kayıt sayısı |
| `merging.observations_merged` | integer | `toplam n_obs - harmonize kayıt sayısı` |
| `merging.max_observations_in_one_record` | integer | Tek kayıttaki en fazla gözlem sayısı |
| `merging.platform_combinations[]` | array | `platforms` (Sensor dizisi) ve `records` (kayıt sayısı). Kayıt sayısına göre azalan sıralanır |
| `vza.records_with_correction` | integer | VZA düzeltmesi uygulanan kayıt sayısı |
| `vza.mean_factor`, `vza.max_factor` | number \| null | Düzeltme faktörlerinin ortalaması ve en büyüğü. Düzeltme yoksa `null` |
| `vza.mean_frp_adjustment_pct` | number \| null | Düzeltme uygulanan kayıtlarda yalnız VZA düzeltmesinin FRP'yi ortalama değiştirme oranı: `100 * (vza_only_frp / frp - 1)`. VZA düzeltmesi MODIS FRP'yi çarparak artırdığı için normalde pozitiftir (pozitif = artış). `frp <= 0` kayıtlar ortalamaya `0` olarak girer |
| `vza.mean_frp_reduction_pct` | number \| null | **Eski alan, geriye uyumluluk için duruyor.** `-mean_frp_adjustment_pct` değerine eşittir, bu yüzden artık normalde negatiftir (negatif = artış). Yeni istemciler `mean_frp_adjustment_pct` kullanmalı |
| `warnings` | array of string | Bkz. "warnings" |
| `report` | object | Bkz. "report" |

### Hatalar

`harmonize` ile aynıdır. Bkz. [harmonize hataları](#get-apiharmonize).

### Notlar

- Önbellek ve zaman aşımı davranışı `harmonize` ile aynıdır.
- VZA istatistikleri harmonize kayıtlardan hesaplanır. VZA katsayıları yer tutucu değerler olabilir; düzeltme büyüklüğü göstergesel okunmalıdır.

## GET /api/stats

Aynı bbox ve zaman penceresi için toplamları, sensör kırılımını, güven ve gündüz/gece dağılımını, FRP özetini, zamansal aralığı ve pipeline metriklerini döndürür. Yanıt kayıt listesi içermez. Parametreler `harmonize` ile aynıdır.

### Parametreler

`harmonize` ile aynıdır. Bkz. [harmonize parametreleri](#get-apiharmonize).

### Örnek istek

```bash
curl -s "http://localhost:3001/api/stats?west=26&south=36&east=45&north=42&days=1"
```

### Örnek yanıt

Bu örnek test düzeneğindeki sahte FIRMS verisinden alındı; sayılar sentetik, yapı gerçek.

Not: `$.sensor_breakdown` dizisindeki 5 öğeden ilk 4 tanesi gösteriliyor.

```json
{
  "totals": {
    "raw_detections": 5,
    "harmonized_records": 3,
    "calendar_months": 1,
    "distinct_platforms": 3
  },
  "sensor_breakdown": [
    {
      "sensor": "ModisTerra",
      "firms_source": "MODIS_NRT",
      "raw_detections": 1,
      "share_pct": 20
    },
    {
      "sensor": "ModisAqua",
      "firms_source": "MODIS_NRT",
      "raw_detections": 1,
      "share_pct": 20
    },
    {
      "sensor": "ViirsSNPP",
      "firms_source": "VIIRS_SNPP_NRT",
      "raw_detections": 3,
      "share_pct": 60
    },
    {
      "sensor": "ViirsNOAA20",
      "firms_source": "VIIRS_NOAA20_NRT",
      "raw_detections": 0,
      "share_pct": 0
    }
  ],
  "confidence_breakdown": {
    "low": 1,
    "nominal": 1,
    "high": 1
  },
  "daynight_breakdown": {
    "day": 3,
    "night": 0
  },
  "frp": {
    "harmonized_sum": 70,
    "harmonized_mean": 23.333333333333332,
    "harmonized_max": 30,
    "harmonized_median": 30
  },
  "temporal": {
    "first_detection": "2026-11-15T10:30:00Z",
    "last_detection": "2026-11-15T11:00:00Z",
    "months_covered": 1
  },
  "pipeline": {
    "transactions_spent": 4,
    "sources_queried": 4,
    "sources_failed": 0
  },
  "warnings": [],
  "report": {
    "per_source": [
      [
        "MODIS_NRT",
        2
      ],
      [
        "VIIRS_NOAA20_NRT",
        0
      ],
      [
        "VIIRS_NOAA21_NRT",
        0
      ],
      [
        "VIIRS_SNPP_NRT",
        3
      ]
    ],
    "failures": [],
    "transactions_spent": 4,
    "uncovered": []
  }
}
```

### Alanlar

| Alan | Tip | Açıklama |
|------|-----|----------|
| `totals.raw_detections` | integer | Ham kayıt sayısı (tip filtresinden sonra) |
| `totals.harmonized_records` | integer | Harmonize kayıt sayısı |
| `totals.calendar_months` | integer | Takvimdeki ay hücresi sayısı (en az bir tespiti olan aylar) |
| `totals.distinct_platforms` | integer | Harmonize kayıtlarda geçen farklı uydu sayısı |
| `sensor_breakdown[]` | array | Beş sensörün her biri için bir öğe (her zaman beş öğe). Alanlar: `sensor`, `firms_source`, `raw_detections`, `share_pct` |
| `sensor_breakdown[].raw_detections` | integer | **Ham** kayıtlara göre sensör sayısı |
| `sensor_breakdown[].share_pct` | number | Sensörün ham kayıtlardaki payı (%). Ham kayıt yoksa `0` |
| `confidence_breakdown` | object | `low`, `nominal`, `high` sayıları. **Harmonize** kayıtlara göre |
| `daynight_breakdown` | object | `day`, `night` sayıları. **Harmonize** kayıtlara göre |
| `frp.harmonized_sum`, `harmonized_mean`, `harmonized_max`, `harmonized_median` | number | `frp_corrected` değerlerinin toplam, ortalama, en büyük ve medyan değerleri. Yalnızca harmonize kayıtlardan hesaplanır |
| `temporal.first_detection`, `last_detection` | RFC 3339 \| null | En erken ve en geç harmonize kayıt zamanı |
| `temporal.months_covered` | integer | `totals.calendar_months` ile aynıdır |
| `pipeline.transactions_spent` | integer | Harcanan FIRMS işlem sayısı |
| `pipeline.sources_queried` | integer | Sorgulanan kaynak sayısı (başarılı ve başarısız dahil) |
| `pipeline.sources_failed` | integer | Başarısız FIRMS isteği sayısı |
| `warnings` | array of string | Bkz. "warnings" |
| `report` | object | Bkz. "report" |

Ham FRP toplamı bu endpoint'te yoktur; `compare` endpoint'indeki `frp.raw_sum` alanına bakın.

### Hatalar

`harmonize` ile aynıdır. Bkz. [harmonize hataları](#get-apiharmonize).

### Notlar

- Önbellek ve zaman aşımı davranışı `harmonize` ile aynıdır.
- Frontend `fetchStats` ile alır.

## GET /api/export

Aynı bbox ve zaman penceresi için harmonize edilmiş hotspot'ları dosya olarak döndürür. Parametreler `harmonize` ile aynıdır ve bir `format` parametresi eklenir.

### Parametreler

`harmonize` ile aynıdır. Ek olarak:

| Ad | Tip | Zorunlu | Varsayılan | Kural |
|----|-----|---------|------------|-------|
| `format` | string | Hayır | `csv` | `csv` veya `geojson` (büyük/küçük harf duyarsız). Başka değer 400 |

`format` doğrulaması bbox ve zaman penceresi doğrulamasından önce yapılır.

### Örnek istek

```bash
curl -s "http://localhost:3001/api/export?west=26&south=36&east=45&north=42&days=1"
curl -s "http://localhost:3001/api/export?west=26&south=36&east=45&north=42&days=1&format=geojson"
```

### Örnek yanıt

#### CSV (varsayılan)

Bu örnek test düzeneğindeki sahte FIRMS verisinden alındı; sayılar sentetik, yapı gerçek.

Yanıt `Content-Type: text/csv; charset=utf-8` ve `Content-Disposition: attachment; filename="pyrosync_26_36_45_42_20261115.csv"` başlıklarıyla döner. Dosya adındaki bbox değerleri istekteki sayıların (f64) çıktısıdır; tarih isteğin UTC günüdür. Satır sonu `\r\n`'dir.

```text
lat,lon,frp_mw,frp_corrected_mw,vza_factor,n_obs,platforms,source,confidence,daynight,acquired_utc,frp_lower_mw,frp_upper_mw
40,33,30,30,1,1,ModisAqua,ModisOnly,Nominal,Day,2026-11-15T10:30:00Z,25.313850194456005,34.68614980554399
39.001,32.001,30,30,,2,ModisTerra|ViirsSNPP,Matched,High,Day,2026-11-15T11:00:00Z,27.6,32.400000000000006
41,34,10,10,,1,ViirsSNPP,ViirsOnly,Low,Day,2026-11-15T11:00:00Z,9.200000000000001,10.8
```

#### GeoJSON

Bu örnek test düzeneğindeki sahte FIRMS verisinden alındı; sayılar sentetik, yapı gerçek.

Yanıt `Content-Type: application/geo+json` ve `Content-Disposition: attachment; filename="pyrosync_26_36_45_42_20261115.geojson"` başlıklarıyla döner.

```json
{
  "features": [
    {
      "geometry": {
        "coordinates": [
          33,
          40
        ],
        "type": "Point"
      },
      "properties": {
        "acquired_utc": "2026-11-15T10:30:00Z",
        "confidence": "Nominal",
        "daynight": "Day",
        "frp_corrected_mw": 30,
        "frp_lower_mw": 25.313850194456005,
        "frp_mw": 30,
        "frp_upper_mw": 34.68614980554399,
        "n_obs": 1,
        "platforms": [
          "ModisAqua"
        ],
        "source": "ModisOnly",
        "vza_factor": 1
      },
      "type": "Feature"
    },
    {
      "geometry": {
        "coordinates": [
          32.001,
          39.001
        ],
        "type": "Point"
      },
      "properties": {
        "acquired_utc": "2026-11-15T11:00:00Z",
        "confidence": "High",
        "daynight": "Day",
        "frp_corrected_mw": 30,
        "frp_lower_mw": 27.6,
        "frp_mw": 30,
        "frp_upper_mw": 32.400000000000006,
        "n_obs": 2,
        "platforms": [
          "ModisTerra",
          "ViirsSNPP"
        ],
        "source": "Matched",
        "vza_factor": null
      },
      "type": "Feature"
    },
    {
      "geometry": {
        "coordinates": [
          34,
          41
        ],
        "type": "Point"
      },
      "properties": {
        "acquired_utc": "2026-11-15T11:00:00Z",
        "confidence": "Low",
        "daynight": "Day",
        "frp_corrected_mw": 10,
        "frp_lower_mw": 9.200000000000001,
        "frp_mw": 10,
        "frp_upper_mw": 10.8,
        "n_obs": 1,
        "platforms": [
          "ViirsSNPP"
        ],
        "source": "ViirsOnly",
        "vza_factor": null
      },
      "type": "Feature"
    }
  ],
  "type": "FeatureCollection"
}
```

### Dosya biçimi

**CSV** sütunları, bu sırayla:

| Sütun | Açıklama |
|-------|----------|
| `lat`, `lon` | Enlem, boylam |
| `frp_mw` | Kaynak FRP (MW) |
| `frp_corrected_mw` | Düzeltilmiş FRP (MW) |
| `vza_factor` | Düzeltme faktörü. Yoksa boş hücre |
| `n_obs` | Birleşen gözlem sayısı |
| `platforms` | Uydular, `|` ile ayrılmış (ör. `ModisTerra|ViirsSNPP`) |
| `source` | `ModisOnly`, `ViirsOnly`, `Matched` |
| `confidence` | `Low`, `Nominal`, `High` |
| `daynight` | `Day`, `Night` |
| `acquired_utc` | RFC 3339, UTC, saniye hassasiyetinde |
| `frp_lower_mw`, `frp_upper_mw` | Güven aralığı sınırları (MW) |

**GeoJSON** bir `FeatureCollection`'dır. Her `Feature` bir `Point` geometrisi taşır; koordinat sırası `[lon, lat]`'tır. `properties` alanları CSV sütunlarının aynısıdır (`frp_mw`, `frp_corrected_mw`, `vza_factor`, `n_obs`, `platforms`, `source`, `confidence`, `daynight`, `acquired_utc`, `frp_lower_mw`, `frp_upper_mw`). Kayıt yoksa `features` boş dizidir.

Bu dosyalarda `calibration` veya ek kalibrasyon alanı yoktur.

### Hatalar

| Durum | Ne zaman | Gövde |
|-------|----------|-------|
| 400 | `format` `csv` veya `geojson` değilse | JSON |
| 400 | Bbox, zaman penceresi veya sorgu parametresi hatalıysa | JSON (`harmonize` ile aynı) |
| 503 | `FIRMS_API_KEY` tanımlı değilse | JSON |
| 502 | Tüm FIRMS istekleri başarısız olduysa | JSON |
| 504 | `REQUEST_TIMEOUT_SECS` süresi dolduysa | JSON |

Örnek (400, geçersiz format):

```json
{
  "error": "Invalid 'format' value 'xml': expected 'csv' or 'geojson'"
}
```

Örnek (503):

```json
{
  "error": "FIRMS_API_KEY is not configured"
}
```

### Notlar

- Önbellek anahtarı `export-csv` veya `export-geojson` rotasıyla ayrılır; aynı bbox ve zaman penceresi için iki biçim ayrı önbelleklenir.
- Aynı istek tekrarlanınca gövde birebir aynı döner; yalnızca `X-Cache: HIT` olur.
- Dosya tek bir yanıttır; sayfalama yoktur. Büyük aralıklar `REQUEST_TIMEOUT_SECS` süresini aşabilir.

## GET /api/calendar

Bir FIRMS ülke arşivinden tarihsel aylık yanma takvimini döndürür. FIRMS anahtarı gerekmez ve FIRMS işlemi harcamaz. Yalnızca yerel arşiv dizini okunur; eksik dosyalar indirilmez.

Arşivde MODIS (Terra ve Aqua birlikte tek dosyada, 2000-2025), VIIRS S-NPP (2012-2025) ve VIIRS NOAA-20 (2018-2025) bulunur. NOAA-21 arşivde yoktur. Bu nedenle takvim, NRT endpoint'lerine göre 2023 sonrası için daha küçük bir uydu filosunu kapsar. `coverage` alanı ve `warnings` bunu belirtir.

### Parametreler

| Ad | Tip | Zorunlu | Varsayılan | Kural |
|----|-----|---------|------------|-------|
| `country` | string | Evet | — | Boş olamaz, en fazla 64 karakter. Yalnızca ASCII harf, rakam, `_` ve `-`. `/api/regions` içindeki `id` ile aynıdır (ör. `Turkey`, `Russian_Federation`) |
| `from` | integer (yıl) | Hayır | `2000` (arşiv alt sınırı) | `from <= to`. Arşiv dışındaki değerler kırpılır |
| `to` | integer (yıl) | Hayır | `2025` (arşiv üst sınırı) | `from <= to`. Arşiv dışındaki değerler kırpılır |
| `west`, `south`, `east`, `north` | number | Hayır | — | Dört parçanın hepsi ya da hiçbiri verilmeli. Kurallar bbox bölümündeki gibidir |

Varsayılan yıl aralığı arşiv kapsamından türetilir: en erken 2000, en geç 2025.

### Örnek istek

```bash
curl -s "http://localhost:3001/api/calendar?country=Turkey&from=2020&to=2021"
curl -s "http://localhost:3001/api/calendar?country=Turkey&from=1990&to=2001"
curl -s "http://localhost:3001/api/calendar?country=Turkey&from=2021&to=2021"
```

### Örnek yanıt

#### Normal yanıt (MISS)

Not: ilgili yıl/ay satırlarına süzüldü (tam yanıt: 11442 bayt); `$.fetch.per_sensor_year` dizisindeki 6 öğeden ilk 4 tanesi gösteriliyor.

```json
{
  "country": "Turkey",
  "years": {
    "requested_from": 2020,
    "requested_to": 2021,
    "effective_from": 2020,
    "effective_to": 2021
  },
  "bbox": null,
  "coverage": {
    "sensors_available": [
      "ModisTerra",
      "ModisAqua",
      "ViirsSNPP",
      "ViirsNOAA20"
    ],
    "sensors_unavailable": [
      "ViirsNOAA21"
    ],
    "note": "The FIRMS country archive packages MODIS, VIIRS SNPP and VIIRS NOAA-20; NOAA-21 is not included, so this calendar covers a smaller fleet than the near-real-time endpoints from 2023 onward."
  },
  "counts": {
    "raw_detections": 141618,
    "harmonized_records": 92285
  },
  "calendar": [
    {
      "year": 2021,
      "month": 7,
      "count": 6688,
      "count_normalized": 1093.1548138917483,
      "effort_omega": 6.118072129408645,
      "mean_frp": 21.780209303715,
      "mean_frp_raw": 22.759430344920197,
      "mean_frp_lower": 21.502882761670893,
      "mean_frp_upper": 22.05753584575911,
      "total_frp": 145666.03982324593,
      "total_frp_lower": 143811.27991005493,
      "total_frp_upper": 147520.79973643692,
      "frp_per_detection_eq": 133.25289151374562,
      "anomaly": null
    },
    {
      "year": 2021,
      "month": 8,
      "count": 8142,
      "count_normalized": 1330.8113777970416,
      "effort_omega": 6.118072129408645,
      "mean_frp": 15.82600700535155,
      "mean_frp_raw": 16.335426348972323,
      "mean_frp_lower": 15.64920622167797,
      "mean_frp_upper": 16.002807789025127,
      "total_frp": 128855.34903757232,
      "total_frp_lower": 127415.83705690203,
      "total_frp_upper": 130294.8610182426,
      "frp_per_detection_eq": 96.8246523792673,
      "anomaly": null
    }
  ],
  "effort": {
    "reliable": true,
    "reason": null,
    "factors": [
      {
        "has_modis": true,
        "viirs_platforms": [
          "ViirsSNPP",
          "ViirsNOAA20"
        ],
        "platforms": [
          "ModisTerra",
          "ModisAqua",
          "ViirsSNPP",
          "ViirsNOAA20"
        ],
        "months": 24,
        "total": 92285,
        "modis_observable": 15084,
        "omega": 6.118072129408645
      }
    ]
  },
  "uncertainty": {
    "confidence": 0.6826894723352727,
    "note": "Harmonized values are model estimates, not ground truth. For MODIS detections, VZA scales raw FRP to MODIS footprint equivalents; VIIRS-era calendar total_frp adds empirically measured pre-VIIRS completeness (k_c) when no VIIRS detections are present in a cell. mean_frp remains native-pixel and is discontinuous at sensor transitions. Default sensor-noise, VZA error, and MODIS scale uncertainty parameters are assumptions, not published values."
  },
  "fetch": {
    "per_sensor_year": [
      {
        "sensor": "ModisTerra",
        "year": 2020,
        "detections": 8208
      },
      {
        "sensor": "ViirsSNPP",
        "year": 2020,
        "detections": 32433
      },
      {
        "sensor": "ViirsNOAA20",
        "year": 2020,
        "detections": 31358
      },
      {
        "sensor": "ModisTerra",
        "year": 2021,
        "detections": 9345
      }
    ],
    "failures": []
  },
  "warnings": [
    "The FIRMS country archive packages MODIS, VIIRS SNPP and VIIRS NOAA-20; NOAA-21 is not included, so this calendar covers a smaller fleet than the near-real-time endpoints from 2023 onward."
  ]
}
```

Aynı istek tekrarlanınca gövde birebir aynı döner; yalnızca `X-Cache: HIT` olur.

#### Yıl aralığı kırpıldı (excerpt)

Not: Bu yanıt bir excerpt'tir; yalnızca `years` ve `warnings` alanları gösteriliyor. Aralık arşiv kapsamının dışına taşınca yıllar `2000..=2001` olarak kırpılır.

```json
{
  "years": {
    "requested_from": 1990,
    "requested_to": 2001,
    "effective_from": 2000,
    "effective_to": 2001
  },
  "warnings": [
    "Requested years 1990..=2001 were clamped to FIRMS country archive coverage (2000..=2001).",
    "The FIRMS country archive packages MODIS, VIIRS SNPP and VIIRS NOAA-20; NOAA-21 is not included, so this calendar covers a smaller fleet than the near-real-time endpoints from 2023 onward."
  ]
}
```

#### NOAA-20 içeren yıl (MISS, excerpt)

Not: `$.calendar` dizisindeki 12 öğeden ilk 4 tanesi gösteriliyor; ilgili yıl/ay satırlarına süzüldü.

```json
{
  "country": "Turkey",
  "years": {
    "requested_from": 2021,
    "requested_to": 2021,
    "effective_from": 2021,
    "effective_to": 2021
  },
  "bbox": null,
  "coverage": {
    "sensors_available": [
      "ModisTerra",
      "ModisAqua",
      "ViirsSNPP",
      "ViirsNOAA20"
    ],
    "sensors_unavailable": [
      "ViirsNOAA21"
    ],
    "note": "The FIRMS country archive packages MODIS, VIIRS SNPP and VIIRS NOAA-20; NOAA-21 is not included, so this calendar covers a smaller fleet than the near-real-time endpoints from 2023 onward."
  },
  "counts": {
    "raw_detections": 69619,
    "harmonized_records": 43338
  },
  "calendar": [
    {
      "year": 2021,
      "month": 1,
      "count": 1913,
      "count_normalized": 346.06857723014446,
      "effort_omega": 5.52780612244898,
      "mean_frp": 6.816263667823399,
      "mean_frp_raw": 6.819461143056285,
      "mean_frp_lower": 6.709486024009027,
      "mean_frp_upper": 6.923041311637772,
      "total_frp": 13039.512396546163,
      "total_frp_lower": 12835.246763929266,
      "total_frp_upper": 13243.778029163059,
      "frp_per_detection_eq": 37.67898403522072,
      "anomaly": null
    },
    {
      "year": 2021,
      "month": 2,
      "count": 1587,
      "count_normalized": 287.09400526097187,
      "effort_omega": 5.52780612244898,
      "mean_frp": 5.122882307145375,
      "mean_frp_raw": 5.112589792060492,
      "mean_frp_lower": 4.9981961256401375,
      "mean_frp_upper": 5.247568488650612,
      "total_frp": 8130.014221439709,
      "total_frp_lower": 7932.137251390898,
      "total_frp_upper": 8327.89119148852,
      "frp_per_detection_eq": 28.31830018202376,
      "anomaly": null
    },
    {
      "year": 2021,
      "month": 3,
      "count": 933,
      "count_normalized": 168.78305413263186,
      "effort_omega": 5.52780612244898,
      "mean_frp": 4.043545747598202,
      "mean_frp_raw": 4.034206859592714,
      "mean_frp_lower": 3.9758836260723394,
      "mean_frp_upper": 4.1112078691240646,
      "total_frp": 3772.628182509123,
      "total_frp_lower": 3709.499423125493,
      "total_frp_upper": 3835.7569418927524,
      "frp_per_detection_eq": 22.35193693997588,
      "anomaly": null
    },
    {
      "year": 2021,
      "month": 4,
      "count": 609,
      "count_normalized": 110.17028935345424,
      "effort_omega": 5.52780612244898,
      "mean_frp": 4.488372617844415,
      "mean_frp_raw": 4.4716365626710495,
      "mean_frp_lower": 4.370567067800736,
      "mean_frp_upper": 4.6061781678880935,
      "total_frp": 2733.4189242672487,
      "total_frp_lower": 2661.6753442906484,
      "total_frp_upper": 2805.162504243849,
      "frp_per_detection_eq": 24.810853636752714,
      "anomaly": null
    }
  ],
  "effort": {
    "reliable": true,
    "reason": null,
    "factors": [
      {
        "has_modis": true,
        "viirs_platforms": [
          "ViirsSNPP",
          "ViirsNOAA20"
        ],
        "platforms": [
          "ModisTerra",
          "ModisAqua",
          "ViirsSNPP",
          "ViirsNOAA20"
        ],
        "months": 12,
        "total": 43338,
        "modis_observable": 7840,
        "omega": 5.52780612244898
      }
    ]
  },
  "uncertainty": {
    "confidence": 0.6826894723352727,
    "note": "Harmonized values are model estimates, not ground truth. For MODIS detections, VZA scales raw FRP to MODIS footprint equivalents; VIIRS-era calendar total_frp adds empirically measured pre-VIIRS completeness (k_c) when no VIIRS detections are present in a cell. mean_frp remains native-pixel and is discontinuous at sensor transitions. Default sensor-noise, VZA error, and MODIS scale uncertainty parameters are assumptions, not published values."
  },
  "fetch": {
    "per_sensor_year": [
      {
        "sensor": "ModisTerra",
        "year": 2021,
        "detections": 9345
      },
      {
        "sensor": "ViirsSNPP",
        "year": 2021,
        "detections": 29484
      },
      {
        "sensor": "ViirsNOAA20",
        "year": 2021,
        "detections": 30790
      }
    ],
    "failures": []
  },
  "warnings": [
    "The FIRMS country archive packages MODIS, VIIRS SNPP and VIIRS NOAA-20; NOAA-21 is not included, so this calendar covers a smaller fleet than the near-real-time endpoints from 2023 onward."
  ]
}
```

### Alanlar

| Alan | Tip | Açıklama |
|------|-----|----------|
| `country` | string | İstenen ülke adı |
| `years.requested_from`, `requested_to` | integer | İstemcinin gönderdiği (veya varsayılan) yıllar |
| `years.effective_from`, `effective_to` | integer | Arşiv kapsamına kırpılmış yıllar |
| `bbox` | object \| null | İstenen bbox. Verilmediyse `null` |
| `coverage.sensors_available` | array of Sensor | Takvimde kullanılan sensörler |
| `coverage.sensors_unavailable` | array of Sensor | Arşivde bulunmayan sensörler (`ViirsNOAA21`) |
| `coverage.note` | string | Arşiv kapsamı açıklaması |
| `counts.raw_detections` | integer | Ham tespit sayısı (tip filtresi ve bbox'tan sonra) |
| `counts.harmonized_records` | integer | Harmonize kayıt sayısı |
| `calendar[]` | array of CalendarCell | Bkz. "CalendarCell" |
| `seasonality` | object | Mevsimsellik: `{ metric: "count", calendar_year: [...], fire_year: [...] }`. Aşağıdaki "seasonality" notuna bakın |
| `effort` | object | Bkz. "effort ve effort.reliable". Bir ülke-yıl dosyası başarısızsa `reliable` `false` olur |
| `uncertainty` | object | Bkz. "Belirsizlik" |
| `fetch.per_sensor_year[]` | array | `sensor`, `year`, `detections` (tip ve bbox filtresinden sonraki sayı). `ModisTerra` tüm MODIS dosyasını (Terra ve Aqua birlikte) temsil eder |
| `fetch.failures[]` | array | `sensor`, `year`, `error` (hata metni) |
| `warnings` | array of string | Bkz. "warnings" |

`count` harmonize kayıt sayısıdır; `count_normalized` ve `effort_omega` MODIS kapsamına göre normalize edilir.

### Hatalar

| Durum | Ne zaman | Gövde |
|-------|----------|-------|
| 400 | `country` boş, 64 karakterden uzun veya izin verilmeyen karakter içeriyorsa | JSON |
| 400 | `from > to` | JSON |
| 400 | Bbox dört parçadan bazıları eksikse | JSON |
| 400 | `country` yoksa veya `from`/`to` tip olarak yanlışsa | JSON: `Failed to deserialize query string: ...` |
| 404 | Yerel arşivde istenen aralıkta hiçbir dosya yoksa | JSON: `No local archive data for country '...'. ...` |
| 500 | Arşiv dizini yapılandırılmamışsa | JSON `FIRMS archive cache directory is unavailable` |
| 504 | `REQUEST_TIMEOUT_SECS` süresi dolduysa | JSON |

Örnek (400, `country` geçersiz):

```json
{
  "error": "Parameter 'country' may contain only ASCII letters, digits, '_' and '-'"
}
```

Örnek (400, yıl sırası ters):

```json
{
  "error": "'from' year (2025) must not be greater than 'to' year (2020)"
}
```

Örnek (400, bbox eksik):

```json
{
  "error": "Invalid bounding box: all four of west, south, east, north are required together"
}
```

Örnek (400, `country` yok):

```json
{
  "error": "Failed to deserialize query string: missing field `country`"
}
```

Örnek (504):

```json
{
  "error": "Request timed out after 300s; reading and harmonizing large multi-year archives can take several minutes. Increase REQUEST_TIMEOUT_SECS or narrow the year range."
}
```

Eksik bir ülke-yıl dosyası istek boyunca başarısız olursa istek 200 döner. Hata `fetch.failures` içinde, `error` alanında bildirilir: `missing locally: archive file <yol> is not in the local archive (read-only client, no download)`. Ayrıca `warnings` içine `Missing local archive data for: ... Run make data ...` uyarısı eklenir ve `effort.reliable` `false` olur. Aynı ülkenin istenen aralıkta hiçbir dosyası yoksa istek 404 döner.

### Notlar

- Takvim yalnızca yerel arşivi okur. Arşiv dosyaları `make data` ile önceden indirilmelidir (yaklaşık 41 GB açılmış).
- Büyük çok yıllı aralıklarda okuma ve harmonizasyon dakikalar sürebilir; `REQUEST_TIMEOUT_SECS` aşılabilir.
- Takvimde yalnızca tespiti olan aylar bulunur. Hiç yangın olmayan aylar ne listede yer alır ne de anomali tabanına girer. `/api/series` ise aynı boş ayları sıfırla doldurur.
- Önbellek anahtarı istenen (`requested_from`, `requested_to`) yıllara ve bbox'a göre oluşur.
- Frontend `fetchCalendar` ile alır.

## GET /api/regions

Yerel arşiv dizinindeki ülkeleri listeler. Liste dizin ilk istekte taranır ve sunucu ömrü boyunca önbellekte tutulur. `/api/series` ve `/api/points` için geçerli `id` değerleri buradan alınır.

### Parametreler

Parametre yok.

### Örnek istek

```bash
curl -s "http://localhost:3001/api/regions"
```

### Örnek yanıt

Not: `$.regions` dizisindeki 11 öğeden ilk 3 tanesi gösteriliyor.

```json
{
  "source": "FIRMS archive country files",
  "max_region_mb": 512,
  "regions": [
    {
      "id": "Armenia",
      "name": "Armenia",
      "modis_years": [
        2000,
        2025
      ],
      "viirs_years": [
        2012,
        2025
      ],
      "noaa20_years": null,
      "size_mb": 2.1,
      "too_large": false,
      "precomputed": false
    },
    {
      "id": "Azerbaijan",
      "name": "Azerbaijan",
      "modis_years": [
        2000,
        2025
      ],
      "viirs_years": [
        2012,
        2025
      ],
      "noaa20_years": null,
      "size_mb": 8.9,
      "too_large": false,
      "precomputed": false
    },
    {
      "id": "Bulgaria",
      "name": "Bulgaria",
      "modis_years": [
        2000,
        2025
      ],
      "viirs_years": [
        2012,
        2025
      ],
      "noaa20_years": null,
      "size_mb": 12.6,
      "too_large": false,
      "precomputed": false
    }
  ]
}
```

### Alanlar

| Alan | Tip | Açıklama |
|------|-----|----------|
| `source` | string | Sabit: `"FIRMS archive country files"` |
| `max_region_mb` | number | `ARCHIVE_MAX_REGION_MB` değeri (MB) |
| `regions[].id` | string | Ülke kimliği. `series`, `points` ve `calendar` `country`/`region` parametrelerinde kullanılır |
| `regions[].name` | string | Görüntüleme adı (`id` içindeki `_` karakterleri boşlukla değiştirilmiş hali) |
| `regions[].modis_years` | [integer, integer] \| null | MODIS dosyalarının `[ilk yıl, son yıl]` aralığı. Dosya yoksa `null` |
| `regions[].viirs_years` | [integer, integer] \| null | VIIRS S-NPP dosyalarının aralığı. Dosya yoksa `null` |
| `regions[].noaa20_years` | [integer, integer] \| null | VIIRS NOAA-20 dosyalarının aralığı. Dosya yoksa `null` |
| `regions[].size_mb` | number | Ülkenin arşiv CSV dosyalarının toplam boyutu (MB, bir ondalık) |
| `regions[].too_large` | boolean | Boyut `ARCHIVE_MAX_REGION_MB` değerini aşıyorsa ve türetilmiş seri yoksa `true`. Bu bölgeler için `series` 413 döner (önceden hesaplanmış seri varsa yanıt verir) |
| `regions[].precomputed` | boolean | Geçerli türetilmiş seri (disk veya paketli) varsa `true` |

Liste ada göre (`name`) sıralıdır.

### Hatalar

| Durum | Ne zaman | Gövde |
|-------|----------|-------|
| 503 | Arşiv dizini okunamıyorsa | JSON: `FIRMS archive is not available: ...` |
| 504 | `ARCHIVE_REQUEST_TIMEOUT_SECS` süresi dolduysa | JSON |

### Notlar

- Dizin ilk çağrıda taranır. Dosyalar sunucu çalışırken eklenirse sunucuyu yeniden başlatmak gerekir.
- Arşiv dizini başlangıçta mevcutsa sunucu açılırken ön ısınma (`warm`) yapılır.
- Frontend `fetchRegions` ile alır.

## GET /api/series

Bir bölgenin yıllık ve aylık serisini döndürür. Yalnızca yerel arşiv ve türetilmiş seri önbelleği okunur; FIRMS anahtarı ve işlem gerekmez.

### Parametreler

| Ad | Tip | Zorunlu | Varsayılan | Kural |
|----|-----|---------|------------|-------|
| `region` | string | Evet | — | `/api/regions` içindeki bir `id`. Boşluklar kırpılır |

### Örnek istek

```bash
curl -s "http://localhost:3001/api/series?region=Turkey"
```

### Örnek yanıt

Not: ilgili yıl/ay satırlarına süzüldü (tam yanıt: 59525 bayt).

```json
{
  "region": "Turkey",
  "filters": {
    "fire_type": [
      0
    ],
    "confidence": "all",
    "area": "FIRMS country archive file: Turkey",
    "sensors": [
      "MODIS Terra+Aqua (archive)",
      "VIIRS S-NPP+NOAA-20 (archive)"
    ]
  },
  "coverage": {
    "modis": {
      "first": "2000-11-01",
      "last": "2025-12-31"
    },
    "viirs": {
      "first": "2012-01-20",
      "last": "2025-12-31"
    }
  },
  "years": [
    {
      "year": 2021,
      "raw_modis": 9345,
      "raw_viirs": 60274,
      "raw_total": 69619,
      "harmonized": 43338,
      "matched": 4363,
      "modis_only": 3477,
      "viirs_only": 35498,
      "interscan_removed": 9787,
      "fleet_merged": 12131,
      "first_date": "2021-01-01",
      "last_date": "2021-12-31",
      "partial": false,
      "modis_first_date": "2021-01-01",
      "modis_last_date": "2021-12-31",
      "viirs_first_date": "2021-01-01",
      "viirs_last_date": "2021-12-31"
    }
  ],
  "months": [
    {
      "year": 2021,
      "month": 7,
      "raw_modis": 1862,
      "raw_viirs": 11900,
      "raw_total": 13762,
      "harmonized": 6690,
      "anomaly_raw": 2.928045264242419,
      "anomaly_harmonized": 2.0377881189013585
    },
    {
      "year": 2021,
      "month": 8,
      "raw_modis": 2067,
      "raw_viirs": 14095,
      "raw_total": 16162,
      "harmonized": 8390,
      "anomaly_raw": 2.1054352467125192,
      "anomaly_harmonized": 1.4434189141778824
    }
  ],
  "cliff": {
    "from_year": 2011,
    "to_year": 2012,
    "raw_from": 18396,
    "raw_to": 31886,
    "raw_change_pct": 73.3311589475973,
    "harmonized_from": 15667,
    "harmonized_to": 25310,
    "harmonized_change_pct": 61.54975426054765
  }
}
```

### Alanlar

| Alan | Tip | Açıklama |
|------|-----|----------|
| `region` | string | Bölge kimliği |
| `filters.fire_type` | [0] | Yalnızca tip 0 (bitki yangını) kayıtları |
| `filters.confidence` | string | Sabit `"all"` |
| `filters.area` | string | Kullanılan arşiv dosyası açıklaması |
| `filters.sensors` | [string, string] | Sabit: MODIS Terra+Aqua ve VIIRS S-NPP+NOAA-20 (arşiv) |
| `coverage.modis` | `{first, last}` \| null | MODIS'in **tüm arşivdeki** gözlem aralığı (ülkeye özgü değil) |
| `coverage.viirs` | `{first, last}` \| null | VIIRS S-NPP ve NOAA-20 aralıklarının birleşimi (tüm arşiv) |
| `years[]` | object | Yıllık özet (aşağıda) |
| `months[]` | object | Aylık seri (aşağıda) |
| `cliff` | object \| null | VIIRS'in ilk yılına göre sensör kayması. Hesaplanamazsa `null` |

`years[]` öğeleri:

| Alan | Tip | Açıklama |
|------|-----|----------|
| `year` | integer | Yıl |
| `raw_modis`, `raw_viirs`, `raw_total` | integer | Ham sayımlar (tip 0). `raw_viirs` S-NPP ve NOAA-20 toplamıdır |
| `harmonized` | integer | Yıl boyunca harmonize edilmiş kayıt sayısı |
| `matched`, `modis_only`, `viirs_only` | integer | Harmonize kaynak dağılımı |
| `interscan_removed` | integer | Tarama çakışması nedeniyle çıkarılan ham kayıt sayısı |
| `fleet_merged` | integer | Kardeş uydu birleştirmesiyle kaldırılan kayıt sayısı |
| `first_date`, `last_date` | `YYYY-MM-DD` \| null | Yılın ilk ve son gözlem tarihi |
| `partial` | boolean | Katkıda bulunan sensörlerden biri yılın tamamını gözlemlemediyse `true` (Ocak 15 - Aralık 15 kapsamı esas alınır) |
| `modis_first_date`, `modis_last_date` | `YYYY-MM-DD` \| null | Yılın MODIS tarihleri |
| `viirs_first_date`, `viirs_last_date` | `YYYY-MM-DD` \| null | Yılın VIIRS tarihleri |

`months[]` öğeleri:

| Alan | Tip | Açıklama |
|------|-----|----------|
| `year`, `month` | integer | Yıl ve ay |
| `raw_modis`, `raw_viirs`, `raw_total` | integer | O ayın ham sayımları |
| `harmonized` | integer | O ay **tek başına** harmonize edilmiş kayıt sayısı |
| `mean_frp` | number \| null | O aydaki harmonize kayıtların (`frp_corrected`) ortalama FRP değeri (MW). Harmonize tespit yoksa veya aralık sonlu değilse `null` |
| `mean_frp_lower`, `mean_frp_upper` | number \| null | `mean_frp` için analitik güven aralığının alt ve üst sınırları (MW; `0 <= mean_frp_lower <= mean_frp <= mean_frp_upper`). Harmonize tespit yoksa veya aralık sonlu değilse `null` |
| `anomaly_raw` | number \| null | `raw_total` için z-skoru |
| `anomaly_harmonized` | number \| null | `harmonized` için z-skoru |

Ay satırları, katkıda bulunan sensörlerden en az birinin gözlem yaptığı her ay için üretilir. Hiç tespit olmayan ama kapsam içindeki aylar sıfır değerle yer alır. Kapsam dışındaki aylar listeye girmez.

Z-skoru, tüm yıllardaki aynı takvim ayı değerleri üzerinden hesaplanır (ortalama ve popülasyon standart sapması). En az 3 değer gerekir; standart sapma sıfırsa `null` döner. Ham ve harmonize seriler ayrı tabanlarla hesaplanır.

`cliff` alanı VIIRS'in ilk yılını (`to_year`) ve bir önceki yılı (`from_year`) karşılaştırır. Değişim yüzdeleri önceki yıl sıfırsa `null` olur.

### Hatalar

| Durum | Ne zaman | Gövde |
|-------|----------|-------|
| 400 | `region` parametresi yok veya boş | JSON: `missing 'region' parameter` |
| 404 | Bölge arşivde yoksa | JSON: `unknown region '...'; see /api/regions` |
| 413 | Bölge `ARCHIVE_MAX_REGION_MB` değerini aşıyor ve seri henüz hesaplanmamış/önbelleğe alınmamış | JSON |
| 500 | Yanıt serileştirilemezse | JSON |
| 503 | Arşiv dizini okunamıyorsa | JSON |
| 504 | `ARCHIVE_REQUEST_TIMEOUT_SECS` süresi dolduysa | JSON: `archive request timed out after 90 s (ARCHIVE_REQUEST_TIMEOUT_SECS)` |

Örnek (400):

```json
{
  "error": "missing 'region' parameter"
}
```

Örnek (404):

```json
{
  "error": "unknown region 'Atlantis'; see /api/regions"
}
```

Örnek (413):

```json
{
  "error": "Region too large for this server: ~24.8M detections / 1936 MB of archive CSV; limit 512 MB (ARCHIVE_MAX_REGION_MB). Run `make precompute` to precompute large regions offline."
}
```

### Notlar

- Yanıt iki kaynaktan birinde türetilmiş seri olarak önceden hesaplanabilir: yazılabilir disk önbelleği (`DERIVED_CACHE_DIR`) veya salt okunur paketli dizin (`BUNDLED_DERIVED_DIR`, varsayılan `data/derived`).
- Yanıtın `X-Series-Source` başlığı kaynağı bildirir: `memory`, `disk`, `bundled` veya `computed`. Gövde her kaynakta aynıdır.
- Bir seri önceden hesaplanmışsa `too_large` sınırı uygulanmaz (413 dönmez). Hesaplama gerektiğinde `ARCHIVE_MAX_REGION_MB` kontrolü yapılır.
- Türetilmiş seri bir damga (şema sürümü, `PipelineConfig` özeti, arşiv dosyalarının yolu ve boyutu) ile doğrulanır. Damga uyuşmazsa seri yeniden hesaplanır ve disk önbelleği güncellenir. Bozuk önbellek dosyası yok sayılır. Yazılamayan bir önbellek dizini isteği engellemez.
- Hesaplanan seri sunucu süreci boyunca bellekte de tutulur (`memory`).
- Önceden hesaplama: `make precompute` (Docker) veya `make precompute-local`. CLI: `pyrosync-server precompute [--region <id>]... [--concurrency <n>] [--force]`. `--force` mevcut geçerli önbelleği yeniden hesaplar.
- `/api/series`, `/api/points` tarafından desteklenen istek bazlı eşik geçersiz kılma parametrelerini (`dedup_time_secs`, `dedup_overlap`, `match_radius_km`, `match_window_secs`, `fleet_radius_km`) **yoksayar**; seriler her zaman sunucunun etkin `PipelineConfig` yapılandırmasıyla önceden hesaplanır ve önbelleğe alınır.
- Frontend `fetchSeries` ile alır.

## GET /api/points

Bir bölgenin tek bir yılına (opsiyonel olarak tek bir ayına) ait ham ve harmonize nokta verilerini döndürür. Yalnızca yerel arşiv okunur.

### Parametreler

| Ad | Tip | Zorunlu | Varsayılan | Kural |
|----|-----|---------|------------|-------|
| `region` | string | Evet | — | `/api/regions` içindeki bir `id` |
| `year` | integer | Evet | — | Bölge için arşivde bir dosya olmalı |
| `month` | integer | Hayır | — (tüm yıl) | `1..=12` |
| `limit` | integer | Hayır | `4000` | En az `1`. `20000`'den büyük değerler `20000`e indirilir. `0` 400 döner |
| `dedup_time_secs` | integer | Hayır | Sunucu yapılandırması (`8`) | `>= 0` (`dedup.max_time_diff_secs` değerini bu istek için ezer) |
| `dedup_overlap` | number | Hayır | Sunucu yapılandırması (`0.3`) | `0.0..=1.0`, sonlu (`dedup.min_overlap_ratio` değerini bu istek için ezer) |
| `match_radius_km` | number | Hayır | Sunucu yapılandırması (`2.0`) | `> 0.0` ve `<= 10.0`, sonlu (`matching.radius_km` değerini bu istek için ezer) |
| `match_window_secs` | integer | Hayır | Sunucu yapılandırması (`10800`) | `>= 0` (`matching.window_secs` değerini bu istek için ezer) |
| `fleet_radius_km` | number | Hayır | Sunucu yapılandırması (`2.0`) | `> 0.0`, sonlu (`fleet.radius_km` değerini bu istek için ezer) |

### Örnek istek

```bash
curl -s "http://localhost:3001/api/points?region=Turkey&year=2021&month=8&limit=4"
curl -s "http://localhost:3001/api/points?region=Turkey&year=2021&month=8&limit=1"
```

### Örnek yanıt

#### limit=4

```json
{
  "region": "Turkey",
  "year": 2021,
  "month": 8,
  "truncated": true,
  "limit": 4,
  "sample_every": 4042,
  "raw": [
    {
      "id": "r2",
      "lat": 37.0533,
      "lon": 27.7028,
      "sensor": "MODIS",
      "frp": 502.2,
      "confidence": "High",
      "acquired": "2021-08-01T00:52:00Z",
      "daynight": "Night",
      "harmonized_id": null
    },
    {
      "id": "r2067",
      "lat": 38.62694,
      "lon": 38.15804,
      "sensor": "VIIRS",
      "frp": 0.48,
      "confidence": "Nominal",
      "acquired": "2021-08-01T00:05:00Z",
      "daynight": "Night",
      "harmonized_id": "h0"
    },
    {
      "id": "r8688",
      "lat": 36.84183,
      "lon": 35.12935,
      "sensor": "VIIRS",
      "frp": 3.01,
      "confidence": "Low",
      "acquired": "2021-08-30T10:43:00Z",
      "daynight": "Day",
      "harmonized_id": "h8084"
    },
    {
      "id": "r12773",
      "lat": 37.28279,
      "lon": 28.40562,
      "sensor": "VIIRS",
      "frp": 6.22,
      "confidence": "Nominal",
      "acquired": "2021-08-04T23:41:00Z",
      "daynight": "Night",
      "harmonized_id": "h4042"
    }
  ],
  "harmonized": [
    {
      "id": "h0",
      "lat": 38.62694,
      "lon": 38.15804,
      "frp": 0.48,
      "frp_corrected": 0.48,
      "frp_lower": 0.4416,
      "frp_upper": 0.5184,
      "vza_factor": null,
      "n_obs": 1,
      "platforms": [
        "ViirsSNPP"
      ],
      "confidence": "Nominal",
      "source": "ViirsOnly",
      "acquired": "2021-08-01T00:05:00Z",
      "daynight": "Night"
    },
    {
      "id": "h4042",
      "lat": 37.28279,
      "lon": 28.40562,
      "frp": 15.1,
      "frp_corrected": 15.1,
      "frp_lower": 13.892,
      "frp_upper": 16.308,
      "vza_factor": null,
      "n_obs": 1,
      "platforms": [
        "ViirsNOAA20"
      ],
      "confidence": "Nominal",
      "source": "ViirsOnly",
      "acquired": "2021-08-04T23:41:00Z",
      "daynight": "Night"
    },
    {
      "id": "h8084",
      "lat": 36.84183,
      "lon": 35.12935,
      "frp": 3.01,
      "frp_corrected": 3.01,
      "frp_lower": 2.7692,
      "frp_upper": 3.2508,
      "vza_factor": null,
      "n_obs": 1,
      "platforms": [
        "ViirsSNPP"
      ],
      "confidence": "Low",
      "source": "ViirsOnly",
      "acquired": "2021-08-30T10:43:00Z",
      "daynight": "Day"
    }
  ],
  "stages": {
    "raw_modis": 2067,
    "raw_viirs": 14095,
    "interscan_removed": 2638,
    "fleet_merged": 3889,
    "matched_pairs": 1245,
    "harmonized": 8390
  },
  "bbox": {
    "west": 26.1566,
    "south": 35.94363,
    "east": 44.5859,
    "north": 41.8025
  }
}
```

#### limit=1

Bu yanıt tek kayıtlı bir örnektir; `limit` değeri 1 olduğunda istek tamamlanır.

```json
{
  "region": "Turkey",
  "year": 2021,
  "month": 8,
  "truncated": true,
  "limit": 1,
  "sample_every": 16162,
  "raw": [
    {
      "id": "r2067",
      "lat": 38.62694,
      "lon": 38.15804,
      "sensor": "VIIRS",
      "frp": 0.48,
      "confidence": "Nominal",
      "acquired": "2021-08-01T00:05:00Z",
      "daynight": "Night",
      "harmonized_id": "h0"
    }
  ],
  "harmonized": [
    {
      "id": "h0",
      "lat": 38.62694,
      "lon": 38.15804,
      "frp": 0.48,
      "frp_corrected": 0.48,
      "frp_lower": 0.4416,
      "frp_upper": 0.5184,
      "vza_factor": null,
      "n_obs": 1,
      "platforms": [
        "ViirsSNPP"
      ],
      "confidence": "Nominal",
      "source": "ViirsOnly",
      "acquired": "2021-08-01T00:05:00Z",
      "daynight": "Night"
    }
  ],
  "stages": {
    "raw_modis": 2067,
    "raw_viirs": 14095,
    "interscan_removed": 2638,
    "fleet_merged": 3889,
    "matched_pairs": 1245,
    "harmonized": 8390
  },
  "bbox": {
    "west": 26.1566,
    "south": 35.94363,
    "east": 44.5859,
    "north": 41.8025
  }
}
```

### Alanlar

| Alan | Tip | Açıklama |
|------|-----|----------|
| `region`, `year` | string, integer | İstek değerleri |
| `month` | integer \| null | İstekteki ay. Verilmediyse `null` |
| `truncated` | boolean | Örnekleme yapıldıysa (`sample_every > 1`) `true` |
| `limit` | integer | Uygulanan limit |
| `sample_every` | integer | `1` ise tam sonuç. Aksi halde her `sample_every`-inci harmonize kayıt tutulur |
| `raw[]` | array | Ham satırlar (aşağıda) |
| `harmonized[]` | array | Harmonize kayıtlar (aşağıda) |
| `stages` | object | Aşamaların sayıları (aşağıda) |
| `bbox` | object \| null | Yıl (veya ay) içindeki **tüm** ham satırların kapsadığı alan. Ham satır yoksa `null`. Örnekleme sonrası döndürülen noktaların alanı değildir |
| `events` | object | Yangın olayı kümeleri (aşağıda). **Tüm** harmonize kayıtlar üzerinden hesaplanır; `limit` örneklemesinden etkilenmez |
| `no_detection_days` | array | Bir sensör ailesinin hiç tespit döndürmediği günler (aşağıda) |
| `config` | object | Bu yanıt üretilirken kullanılan etkin `PipelineConfig` (varsayılan sunucu yapılandırması + istek geçersiz kılmaları; şema `/api/config` ile aynıdır) |
| `config_overridden` | boolean | İstek bazlı eşik parametrelerinden en az biri verildiyse `true`, aksi halde `false` |

`raw[]` öğeleri:

| Alan | Tip | Açıklama |
|------|-----|----------|
| `id` | string | `r<n>`. `n`, MODIS ve VIIRS satırlarının birleşik listesindeki sıra numarasıdır (örnekleme öncesi) |
| `lat`, `lon`, `frp` | number | Konum ve FRP |
| `sensor` | string | `"MODIS"` veya `"VIIRS"` (Sensor enum'u değil). VIIRS, S-NPP ve NOAA-20 satırlarını birlikte kapsar |
| `confidence` | Confidence | Güven |
| `acquired` | RFC 3339 zaman | Gözlem zamanı |
| `daynight` | DayNight | Gündüz/gece |
| `harmonized_id` | string \| null | Bağlı harmonize kaydın kimliği (`h<n>`). Satır interscan çakışması olarak çıkarıldıysa `null` |

`harmonized[]` öğeleri: `id` (`h<n>`) ve harmonize kaydın tüm alanları (bkz. "Harmonize edilmiş kayıt"). Ek olarak `event_id` (integer \| null) kaydın ait olduğu olayın `events.items[].id` değeridir; "watch" noktalarında `null`. Olay listesi 1000'e kırpıldıysa (`items_truncated`) `event_id` listede bulunmayan bir olaya işaret edebilir. `n`, yıl (veya ay) boyunca üretilen harmonize listesindeki sıradır; örnekleme öncesi numaralandırılır.

`stages` alanı tüm yıl veya ay üzerinden hesaplanır, yalnızca döndürülen örnek üzerinden değil:

| Alan | Açıklama |
|------|----------|
| `raw_modis`, `raw_viirs` | Ham satır sayıları |
| `interscan_removed` | Tarama çakışması ile çıkarılan satırlar |
| `fleet_merged` | Kardeş uydu birleştirmesiyle kaldırılan kayıtlar |
| `matched_pairs` | MODIS-VIIRS eşleşme çifti sayısı |
| `harmonized` | Harmonize kayıt sayısı |

#### Yangın olayları (`events`)

Kümeleme `harmonized` çıktısının tamamı üzerinde, örneklemeden önce çalışır. Yöntem eg-firetrack referans projesinin (`events.py`) kuralını izler: harmonize tespitler üzerinde DBSCAN (`eps = 2 km`, `min_samples = 3`) ve DBSCAN'in dışında kalan seyrek tespitler için "ardışık 2 gün" kuralı; tek başına kalan 1-2 tespit olay değil "watch" noktasıdır. DBSCAN uzay-zamanda (ST-DBSCAN) çalışır: iki tespit komşudur, eğer haversine mesafesi `eps_km`'den küçük veya eşitse **ve** UTC takvim günü farkı `max_day_gap`'i (1) aşmıyorsa. Böylece aynı yer farklı hafta/yıllarda birleşmez. Antimeridyen (±180° boylam) komşu aramasında sarılmaz.

| Alan | Tip | Açıklama |
|------|-----|----------|
| `events.params` | object | `eps_km` (2.0), `min_samples` (3), `max_day_gap` (1) |
| `events.count` | integer | Toplam olay sayısı (kırpmadan önce) |
| `events.watch_points` | integer | Hiçbir olaya girmeyen harmonize tespit sayısı |
| `events.items` | array | Olaylar, `id` sırasıyla. En fazla 1000; fazlası varsa tespit sayısı en büyük olanlar tutulur |
| `events.items_truncated` | boolean | `count` 1000'i aştıysa `true` |

`events.items[]` öğeleri:

| Alan | Tip | Açıklama |
|------|-----|----------|
| `id` | integer | En erken tespit zamanına göre (eşitlikte girdi sırası) atanan kararlı kimlik |
| `kind` | string | `"dense"` (DBSCAN kümesi) veya `"consecutive_days"` (yoğunluk eşiğine ulaşmayan ama >= 2 ayrı UTC gününde süren tespit grubu) |
| `detections` | integer | Olaydaki harmonize tespit sayısı |
| `start`, `end` | RFC 3339 zaman | İlk ve son tespit |
| `days` | integer | Tespit bulunan ayrı UTC günü sayısı |
| `centroid_lat`, `centroid_lon` | number | Aritmetik ortalama konum |
| `bbox` | object | `west`, `south`, `east`, `north` |
| `total_frp` | number | `frp_corrected` toplamı, MW |

#### Tespit olmayan günler (`no_detection_days`)

İstenen dönem (ay veya tüm yıl) içinde, bir sensör ailesinin (`"Modis"`; `"Viirs"` = SNPP + NOAA-20) kapsama aralığında olup bölgede hiç tespit döndürmediği günler. eg-firetrack'in `no_detection_days` metaverisine karşılık gelir. Dönem, ailenin arşiv kapsamıyla (bölgenin o yıl için dosyası olan sensörlerin genel arşiv tarih aralığı; VIIRS için SNPP ve NOAA-20 aralığının birleşimi) kesiştirilir; kapsam dışındaki günler listelenmez. Liste `(date, family)` sırasındadır.

| Alan | Tip | Açıklama |
|------|-----|----------|
| `date` | `YYYY-MM-DD` | Gün |
| `family` | string | `"Modis"` veya `"Viirs"` |
| `reason` | string | `"known_outage"` veya `"no_detections"` |
| `other_family_detected` | boolean | Diğer ailenin o gün tespiti varsa `true` (veri boşluğu olduğuna dair ipucu) |

`known_outage` yalnızca MODIS için ve Remy & Kaiser (2014, ACP, "Daily global fire radiative power from MODIS", Bölüm 4) tarafından belgelenen Terra MOD14 veri arızası pencerelerinde verilir: 6-17 Ağustos 2000, 16 Haziran-2 Temmuz 2001, 21-29 Mart 2002 (hepsi Aqua öncesi; gerçek sıfır yangın değil, veri arızası). Bunun dışındaki her gün `no_detections` olur.

**Uyarı:** `no_detections` kesin bir kesinti kanıtı değildir. "Bu bölge-günde tespit yok" demektir ve gerçek bir sıfır-yangın gününü bir veri boşluğundan tek başına ayıramaz; `other_family_detected: true` yalnızca boşluk olasılığına işaret eden bir ipucudur. Tespit günleri arşivdeki yaban yangını (`type = 0`) satırlarından çıkarılır.

#### Örnekleme davranışı

Ham satır sayısı `limit` değerini aşarsa sunucu, her `k`-inci harmonize kaydı (zaman sırasıyla) bağlı tüm ham satırlarıyla birlikte tutar. Bağlantısız (interscan çakışması) ham satırlar da aynı oranda örneklenir. `k`, ceil(ham satır / limit) değerinden başlayıp sonuç `limit` içine sığana kadar artırılan en küçük adımdır. Hiçbir adım sığmıyorsa (ör. tek bir harmonize kaydın kendi satırları bile `limit`i aşıyorsa) sunucu zaman sırasına göre açgözlü (greedy) bir seçimle `limit` içinde kalır. Örnekleme için bir üst sınır vardır; istek sonsuz döngüye girmez.

Örneklenmeyen bir istekte `truncated` `false` ve `sample_every` `1` olur.

### Hatalar

| Durum | Ne zaman | Gövde |
|-------|----------|-------|
| 400 | `region` yoksa | JSON: `missing 'region' parameter` |
| 400 | `year` yoksa | JSON: `missing 'year' parameter` |
| 400 | `year`, `month`, `limit` veya eşik parametrelerinden biri sayı değilse | JSON: `invalid '<param>' value '...'` |
| 400 | `month` `1..=12` dışındaysa | JSON: `'month' must be within 1..=12` |
| 400 | `limit=0` | JSON: `'limit' must be at least 1` |
| 400 | Eşik geçersiz kılma değerleri aralık dışındaysa (`dedup_time_secs < 0`, `dedup_overlap` `0.0..=1.0` dışında, `match_radius_km` `(0.0, 10.0]` dışında, `match_window_secs < 0`, `fleet_radius_km <= 0` veya sonlu değil) | JSON: doğrulama hatası (ilgili alan adını içerir, ör. `matching.radius_km must be <= 10, got 50`) |
| 404 | Bölge bilinmiyorsa | JSON |
| 404 | Bölge için o yılda dosya yoksa | JSON: `the archive has no Turkey file for 1999` |
| 413 | Seçilen **yıl** `ARCHIVE_MAX_REGION_MB` değerini aşıyorsa | JSON: `Region year 2021 too large for this server: ...` |
| 503 | Arşiv dizini okunamıyorsa | JSON |
| 504 | `ARCHIVE_REQUEST_TIMEOUT_SECS` süresi dolduysa | JSON |

Örnek (400, `month` aralık dışı):

```json
{
  "error": "'month' must be within 1..=12"
}
```

Örnek (400, `limit=0`):

```json
{
  "error": "'limit' must be at least 1"
}
```

Örnek (404, arşivde o yıl yok):

```json
{
  "error": "the archive has no Turkey file for 1999"
}
```

### Notlar

- Boyut sınırı yıl bazında uygulanır. Bölgenin toplam boyutu sınırı aşsa bile istenen yıl sınırın altındaysa istek yanıt verir.
- Yıl verisi ilk istekte okunur ve bir yıl önbelleğinde tutulur. Önbelleğin toplam bütçesi 4.000.000 satırdır. Bütçe aşılınca en az yakın zamanda kullanılan (LRU) yıl önce çıkarılır. Tek bir yılın satır sayısı bütçeyi aşıyorsa o yıl önbelleğe alınmaz.
- `month` verildiğinde yalnızca o ayın satırları işlenir.
- İstek bazlı eşik parametreleri (`dedup_time_secs`, `dedup_overlap`, `match_radius_km`, `match_window_secs`, `fleet_radius_km`) yalnızca bu isteğin harmonize işlemini etkiler ve yanıt önbelleğine yazılmaz; `/api/series` bu parametreleri yoksayar.
- Frontend `fetchPoints` ile alır.

## Hata kodları

| Kod | Hangi endpoint'ler | Ne zaman | Gövde |
|-----|--------------------|----------|-------|
| 400 | harmonize, compare, stats, export, calendar | Bbox geçersiz, `days` aralık dışı, `start`/`end` eksik veya tarih biçimi yanlış, `days` ile `start`/`end` birlikte, `format` geçersiz, calendar `country` geçersiz, `from > to`, bbox parçaları eksik | JSON `{"error": "..."}` |
| 400 | harmonize, compare, stats, export, calendar | Zorunlu sorgu parametresi yok veya tipi hatalı | JSON: `Failed to deserialize query string: ...` |
| 400 | series, points | `region`/`year` eksik; `year`/`month`/`limit` veya eşik parametreleri sayı değil; `month` aralık dışı; `limit=0`; eşik geçersiz kılma değeri aralık dışı | JSON |
| 404 | (herhangi) | `/api/` altında bilinmeyen yol: `Unknown API endpoint: /api/...` | JSON |
| 404 | calendar | Yerel arşivde istenen aralıkta hiçbir dosya yok | JSON |
| 404 | series, points | Bilinmeyen bölge; bölge için o yılda arşiv dosyası yok | JSON |
| 413 | series (hesaplama gerektiğinde), points | Bölge (series) veya seçilen yıl (points) `ARCHIVE_MAX_REGION_MB` değerini aşıyor | JSON |
| 500 | calendar | Arşiv dizini yapılandırılmamış | JSON |
| 500 | genel | İç hata (ör. yanıt serileştirme, arşiv ayrıştırma, worker hatası) | JSON |
| 502 | availability | FIRMS availability tablosu alınamadı | JSON |
| 502 | harmonize, compare, stats, export | Tüm FIRMS istekleri başarısız oldu (availability dahil) | JSON `Upstream FIRMS request failed: ...` |
| 503 | availability, harmonize, compare, stats, export | `FIRMS_API_KEY` tanımlı değil | JSON `FIRMS_API_KEY is not configured` |
| 503 | regions, series, points | Arşiv dizini okunamadı (`FIRMS archive is not available: ...`) | JSON |
| 504 | harmonize, compare, stats, export | `REQUEST_TIMEOUT_SECS` aşıldı | JSON |
| 504 | calendar | `REQUEST_TIMEOUT_SECS` aşıldı | JSON |
| 504 | regions, series, points | `ARCHIVE_REQUEST_TIMEOUT_SECS` aşıldı | JSON |

Hata gövdesi her zaman `{"error": "..."}` biçimindedir.

## API'yi etkileyen ortam değişkenleri

| Değişken | Varsayılan | Etkisi |
|----------|------------|--------|
| `FIRMS_API_KEY` | — | NASA FIRMS anahtarı. Tanımlı değilse `availability`, `harmonize`, `compare`, `stats` ve `export` 503 döner. `calendar`, `regions`, `series` ve `points` çalışmaya devam eder |
| `AREA_CACHE_DIR` | `.cache/firms-area` | FIRMS Area API pencere önbelleği. Önbellek isabeti FIRMS işlemi harcamaz. Boş değer önbelleği kapatır |
| `AREA_CACHE_NRT_TTL_SECS` | `600` | NRT pencerelerinin geçerlilik süresi (sn). SP pencereleri süresizdir. `0` NRT pencerelerini önbellekten hiç sunmaz. Geçersiz değer varsayılanı kullanır |
| `ARCHIVE_CACHE_DIR` | `.cache/firms-archive` | FIRMS ülke arşivi dizini. `calendar`, `regions`, `series` ve `points` buradan okur. Sunucu arşiv indirmez; veriyi `make data` hazırlar. `FIRMS_ARCHIVE_DIR` geri dönüş adıdır (`ARCHIVE_CACHE_DIR` boşsa kullanılır) |
| `DERIVED_CACHE_DIR` | `.cache/pyrosync-derived` | Türetilmiş `/api/series` önbelleğinin yazılabilir dizini |
| `BUNDLED_DERIVED_DIR` | `data/derived` | Salt okunur, depoyla gelen türetilmiş seri dizini |
| `ARCHIVE_MAX_REGION_MB` | `512` | Bir bölgenin (series) veya yılın (points) arşiv boyutu bu değeri (MB) aşarsa 413 döner. `regions` içinde `too_large` ayarlanır. `0` veya geçersiz değer varsayılanı kullanır |
| `ARCHIVE_REQUEST_TIMEOUT_SECS` | `90` | `regions`, `series` ve `points` için zaman aşımı (sn). Aşılırsa 504 döner. `0` veya geçersiz değer varsayılanı kullanır |
| `CACHE_BUDGET_MB` | `128` | Yanıt önbelleğinin MB bütçesi. `0` önbelleği kapatır. Geçersiz değer varsayılana döner |
| `REQUEST_TIMEOUT_SECS` | `300` | `harmonize`, `compare`, `stats`, `export` ve `calendar` için zaman aşımı (sn). `0` zaman aşımını kapatır. Geçersiz değer varsayılana döner |
| `CORS_ALLOWED_ORIGINS` | — | Virgülle ayrılmış izinli origin listesi. Tanımlıysa yalnızca bunlara `GET` ve `content-type` izni verilir. Geçersiz girdiler log'a yazılır ve atlanır. Tanımlı değilse CORS geneldir |
| `PYROSYNC_CONFIG` | — | Pipeline yapılandırması TOML dosyasının yolu. Verilirse dosya zorunludur; yoksa hata ile başlangıç durur. Verilmezse çalışma dizinindeki `pyrosync.toml` varsa kullanılır |
| `PYROSYNC_DEDUP_MAX_TIME_DIFF_SECS` | `8` | Interscan duplikat zaman farkı (sn). TOML değerini ezer |
| `PYROSYNC_DEDUP_MIN_OVERLAP_RATIO` | `0.30` | Interscan duplikat minimum örtüşme oranı |
| `PYROSYNC_FLEET_VIIRS_WINDOW_SECS` | `3600` | VIIRS filo birleştirme penceresi (sn) |
| `PYROSYNC_FLEET_MODIS_WINDOW_SECS` | `10800` | MODIS filo birleştirme penceresi (sn) |
| `PYROSYNC_FLEET_RADIUS_KM` | `2.0` | Filo birleştirme yarıçapı (km) |
| `PYROSYNC_MATCH_RADIUS_KM` | `2.0` | MODIS/VIIRS eşleştirme yarıçapı (km, en fazla 10) |
| `PYROSYNC_MATCH_WINDOW_SECS` | `10800` | MODIS/VIIRS eşleştirme zaman penceresi (sn) |
| `PYROSYNC_VZA_A`, `PYROSYNC_VZA_B`, `PYROSYNC_VZA_C` | `0.2017`, `0.0168`, `1.0` | MODIS VZA düzeltme polinomu katsayıları |
| `PORT` | `3001` | Sunucu portu. Sunucu `0.0.0.0:PORT` üzerinde dinler |
| `STATIC_DIR` | `web/dist` | SPA build dizini. Docker'da `/app/static` |
| `RUST_LOG` | `pyrosync=info` | Log seviyesi |

Pipeline yapılandırması şu sırayla oluşturulur: kodda tanımlı varsayılanlar, ardından TOML dosyası (`PYROSYNC_CONFIG` veya `./pyrosync.toml`), ardından `PYROSYNC_*` ortam değişkenleri. TOML'da bilinmeyen bir anahtar hata verir. Değerler doğrulanır (ör. `matching.radius_km` en fazla 10); geçersiz bir yapılandırma sunucunun başlamasını engeller. Etkin değerler `GET /api/config` ile okunur. Şablon: `pyrosync.example.toml`.

`ARCHIVE_SET_FILE` sunucu tarafından okunmaz; yalnızca `archive_seed` araçı kullanır.

### Türetilmiş seri önbelleği (ops)

`/api/series` sonuçları iki katmanda saklanabilir: önceden hesaplanmış dosyalar (`make precompute`) ve istek sırasında yazılan disk önbelleği. Damga uyuşmazlığında (arşiv dosyası değişmiş, yapılandırma değişmiş, şema sürümü farklı) seri yeniden hesaplanır. Bu davranış istemciye yalnızca `X-Series-Source` başlığı ve yanıt süresi olarak yansır.

## Bilinen sorunlar

- **FIRMS availability tablosu süreç başına bir kez alınır.** Tablo bir `OnceCell` içinde tutulur. Sunucu UTC gece yarısını geçtikten sonra en yeni günler `uncovered` olarak raporlanabilir ve `/api/availability` eski tarihleri gösterir. Çözüm sunucuyu yeniden başlatmaktır.
- **Takvimde yangınsız aylar yoktur.** `/api/calendar` yalnızca en az bir tespiti olan ayları döndürür. Bu nedenle yangınsız aylar ne takvimde görünür ne de anomali tabanına girer. Buna karşın `/api/series` aynı ayları sıfırla doldurur.

## Frontend notu

`web/src/services/api.ts` bu dokümandaki wire formatını uygulamanın camelCase tiplerine dönüştürür. Ayrıca aşağıdaki fonksiyonlar bu endpoint'leri çağırır: `fetchRegions`, `fetchSeries`, `fetchPoints`, `fetchHarmonize`, `fetchCompare`, `fetchStats`, `fetchCalendar`. Bu doküman sunucunun gönderdiği wire formatını tanımlar.

`/api/health`, `/api/config`, `/api/availability` ve `/api/export` şu anda frontend tarafından kullanılmaz.
