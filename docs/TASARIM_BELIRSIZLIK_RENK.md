# PyroSync — Belirsizlik Gösterimi ve Erişilebilir Renk Skalası Tasarımı (C7)

> **Durum:** Tasarım spesifikasyonu (C7) · **Hedef devirler:** K3 (Belirsizlik + Sayısal Sunum) ve K4 (Renk Skalası + Quantile + Palet Seçici)
> **Temel referanslar:** [docs/RENK_DENETIMI.md](RENK_DENETIMI.md), [docs/TASARIM_SISTEMI.md](TASARIM_SISTEMI.md), [docs/TASARIM_KONSEPT.md](TASARIM_KONSEPT.md), [docs/UX_YAZIM.md](UX_YAZIM.md), [web/src/styles/tokens.css](../web/src/styles/tokens.css)

Bu belge, PyroSync arayüzünde armonize FRP (`frp_corrected`) ve aylık ortalama FRP (`mean_frp`) değerlerine eşlik eden model belirsizliği aralıklarının tüm görselleştirmelerde nasıl gösterileceğini ve CVD (renk görme eksikliği) uyumlu renk skalası mimarisini tanımlar. Yalnızca tasarım ve arayüz sözleşmesini belirler; kod değişikliği K3 ve K4 aşamalarında uygulanacaktır.

---

## 0. Mevcut Kod Gerçeği ve Tasarım Kısıtları

Tasarım kararlarının uygulanabilir kalması için arka uç (G8–G9) ile ön yüz arasındaki mevcut durum ayrımı aşağıda netleştirilmiştir:

| Katman | Mevcut Durum | Eksik Halka (K3 / K4 Kapsamı) |
| :--- | :--- | :--- |
| Rust çekirdeği (`crates/core/src/uncertainty.rs`, `models.rs`, `calibration.rs`, `harmonize.rs`, `export.rs`) | `HarmonizedHotspot` üzerinde `frp_lower`, `frp_upper`, `vza_factor`, `calibration_ratio`, `n_obs`; `CalendarCell` üzerinde `mean_frp`, `mean_frp_lower`, `mean_frp_upper` üretilir. CSV/GeoJSON dışa aktarımı `frp_lower_mw` ve `frp_upper_mw` sütunlarını yazar. | — (Backend belirsizlik matematiği tamamlandı) |
| API sunucusu (`crates/server/src/main.rs`) | `/api/points` `HarmonizedHotspot` alanlarını (`#[serde(flatten)]`) döndürür; `/api/calendar` `CalendarCell` + `UncertaintyMetadata` (`confidence`, `note`) döndürür; `/api/harmonize` `uncertainty` nesnesini döndürür. | `/api/series` (`MonthEntry` / `YearEntry`) yanıtı yalnızca tespit sayımları taşır; `mean_frp` ve `mean_frp_lower`/`mean_frp_upper` henüz seri ucunda yer almaz. |
| Ön yüz (`web/src/types/index.ts`, `CliffChart.tsx`, `BurnCalendar.tsx`, `FlatMap.tsx`, `GlobeMap.tsx`, `PointDetail.tsx`, `scales.ts`) | `CliffChart` elle yazılmış SVG'dir (Recharts değildir) ve yıllık tespit sayımlarını çizer. `BurnCalendar` 12 aylık **radyal yıl saati** SVG'sidir ve sayım/anomali gösterir. `MapLegend` ve `scales.ts` sabit log-aralıklı `FRP_BREAKS = [5, 10, 20, 40, 80, 160, 320]` kullanır. `MethodsDrawer` "Display / Görünüm" bölümünde yalnızca tema (`dark`/`light`) ve sunum modunu barındırır. | TypeScript tiplerinde `frpLower`/`frpUpper`/`meanFrpLower`/`meanFrpUpper` tanımlı değildir; belirsizlik bantları/taramaları/opaklıkları, quantile gruplama ve palet seçici henüz bağlı değildir. |

**Bağlayıcı ekran ve erişilebilirlik kısıtları:**
- **Masaüstü:** `1366×768` (harita merkezde, alt panelde `~220px` `CliffChart` ve radyal `BurnCalendar`, sol altta `MapLegend`, sağda çekmeceler).
- **Mobil:** `375×812` (tek sütun istif, alt sayfa `PointDetail`, daraltılabilir `MapLegend`).
- **Temalar:** Koyu tema (`:root`, varsayılan) ve açık tema (`:root[data-theme="light"]`).
- **Hareket azaltma:** `@media (prefers-reduced-motion: reduce)` ve `data-presentation="true"` altında geçişler anlıktır (`--dur-reduced: 150ms` veya `0ms`, sürekli parıltı/atış animasyonu yok).
- **Baskı / Gri tonlama:** `@media print` ve akromatopsi koşullarında anlam yalnızca renge yaslanmaz; şekil, çizgi biçimi (kesikli/sürekli), SVG tarama deseni ve metin eşlik eder.
- **Ses tonu ([docs/UX_YAZIM.md](UX_YAZIM.md)):** "Sakin bilim insanı" — ünlem, emoji ve pazarlama sıfatları yasak; sayısal birimden önce kesintisiz boşluk (`\u00A0MW`), TR'de virgül ondalık ve başa `%`, EN'de nokta ondalık ve sona `%`, zaman damgaları UTC.

---

## 1. Belirsizlik Gösterimi — Grafik Türüne Göre Karar Tablosu

Model belirsizliği her görünümde yalnızca **Harmonized** (armonikleştirilmiş) katmanda ve FRP/kalibrasyon türevli metriklerde görünür. **Raw** (ham) katman, sensörlerin doğrudan bildirdiği nokta gözlemlerini gösterdiği için model güven aralığı taşımaz; bu ayrım kullanıcıya açıkça belirtilir.

| Bileşen / Grafik | Görsel Kodlama | Token(lar) | Raw vs Harmonized Davranışı | `prefers-reduced-motion` | Light / Dark Uyumu | `1366×768` & `375×812` |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **1. Cliff zaman serisi** (`CliffChart.tsx` — elle yazılmış SVG) | Ortalama FRP veya kalibre edilmiş seri çizgisi etrafında yarı saydam **güven bandı (`<path>` alt–üst poligonu)**; geniş aralık (`> %30`) dönemlerinde bandın üstünde ince kesikli üst sınır çizgisi. | Mevcut: `--color-accent`, `--slice-harm`, `--color-border-subtle`<br>Yeni öneri: `--color-uncertainty-band`, `--color-uncertainty-band-stroke` | **Raw:** Bant çizilmez (yalnızca noktalı MODIS çizgisi ve VIIRS kare/çizgisi; `modisArea` `fill-opacity: 0.1` ham sayım alanı olarak kalır).<br>**Harmonized:** `mean_frp_lower`–`mean_frp_upper` arası bant çizilir; `morph` geçişiyle yumuşak görünür. | `morph` animasyonu kapanır (`--dur-reduced`); bant doğrudan son geometrisinde belirir, sürüklenmez. | Dark'ta düşük opaklık nötr gri ton (`0.22`), Light'ta koyulaştırılmış şeffaf dolgu (`0.16`) — arka plan çizgilerini yutmaz. | `1366×768`: `220px` panelde ana çizgiyle birlikte.<br>`375×812`: bant kalınlığı korunur; hover yerine dokunmatik dikey kılavuz (`pointer`) ile aralık metni okunur. |
| **2. Çubuk / Histogram** (`CountDelta` dağılımı veya dönem FRP karşılaştırma çubukları) | Çubuk tepesinde merkezlenmiş **hata çubuğu (`I`-şekilli dikey whisker + yatay uç şapkalar)** (`1.5px` çizgi, `6px` şapka). Alt ve üst uç çubuk sınırından taşmaz; sıfır altında `0 MW`'a kırpılır (`lower.max(0.0)` ile uyumlu). | Mevcut: `--color-text`, `--color-text-muted`, `--color-surface`<br>Yeni öneri: `--uncert-errorbar-stroke` | **Raw:** Hata çubuğu yok (ham tespit sayımları kesin sayılır).<br>**Harmonized:** Ortalama FRP / kalibrasyonlu çubuklarda `mean_frp_lower`–`mean_frp_upper` çubuğu çizilir; salt tespit sayımı farkında (`CountDelta`) hata çubuğu gösterilmez, bunun yerine model aralığı dipnotu verilir. | Hata çubukları çubukla birlikte animasyonsuz yerleşir (`transition: none`). | Çubuk içi koyu/açık geçişinde çift kontrastlı stroke (ana gövde + `1px` zıt kılıf), her iki temada `>= 3:1` görünürlük. | `1366×768`: 6px uç şapka.<br>`375×812`: Dar sütunlarda çakışmayı önlemek için şapka `4px`, minimum çubuk genişliği `14px`. |
| **3. Takvim / Radyal Yıl Saati** (`BurnCalendar.tsx` — 12 aylık dilimler) | Dilim başına **göreli aralık genişliği** `relWidth = (mean_frp_upper - mean_frp_lower) / mean_frp` hesaplanır. **`relWidth > 0.30` (%30 eşiği)** olduğunda dilim dolgusu üzerine `45°` **SVG `<pattern>` diyagonal tarama** uygulanır ve dilim dış kenarlığı `stroke-dasharray: 3 2` kesikli çizgiye döner. (Boş ay `stroke-dasharray: 2 2` ile karışmaz; boş ayın dolgusu `anom-none` ve kenarı incedir.) | Mevcut: `--anom-neg-3..pos-3`, `--anom-none`, `--slice-harm`, `--color-border-subtle`<br>Yeni öneri: `--uncert-hatch-stroke`, `--uncert-slice-border` | **Raw:** Dilimler ham sayım/ham ortalama taşır; tarama uygulanmaz.<br>**Harmonized:** `mean_frp_lower` ve `mean_frp_upper` mevcutsa eşik kontrolü çalışır. Eşik altında düz dolgu + düz kenar; `> 0.30` ise tarama + kesikli kenar + `CalendarTooltip` uyarı satırı. | Tarama statiktir (animasyonlu desen kaydırması yok). | Dark'ta tarama çizgisi `rgba(255,255,255,0.28)`, Light'ta `rgba(17,24,39,0.26)`; alttaki PuOr anomali tonunu boğmaz. | `1366×768`: Tarama aralığı `6px` adım, `1.25px` çizgi.<br>`375×812`: Yarıçap küçüldüğü için tarama adımı `5px`; küçük dilimlerde kesikli kenarlık birincil sinyal kalır. |
| **4. Harita Noktaları** (`FlatMap.tsx` 2D Canvas & `GlobeMap.tsx` WebGL2 `THREE.Points`) | **Seçim: Nicemlenmiş 3 kademeli opaklık kodlaması** (gerekçe Bölüm 1.1'de). Göreli genişliğe göre: `<= 0.20` -> tam opaklık (`1.00`), `0.20–0.40` -> orta (`0.72`), `> 0.40` -> düşük (`0.48`). Seçili/vurgulanan nokta daima `1.00` opaklıkta çizilir. | Mevcut: `--frp-1..8`, `--map-halo-light`, `--map-duplicate-ring`<br>Yeni öneri: `--uncert-alpha-tight` (`1.0`), `--uncert-alpha-mid` (`0.72`), `--uncert-alpha-wide` (`0.48`) | **Raw:** Opaklık yalnızca yakınlaştırma yoğunluğuna (`modisAlpha` / `viirsAlpha`) bağlıdır; belirsizlik kodlaması yok.<br>**Harmonized:** Her nokta FRP kovasına (`bucket`) + 3 belirsizlik kademesine göre `bucket × 3` grupta (Canvas `Path2D`) veya tek `aUncert` float attribute'uyla (WebGL2) çizilir. | `uGlow` yavaş sönümlenmesi kapatılır; opaklık basamakları statiktir. | `MIN_ALPHA = 0.48` ve zorunlu `--map-halo-light` / `uCasing` çevrelemesi sayesinde en belirsiz nokta bile koyu ve açık zeminde kaybolmaz. | `1366×768`: Yoğun kümelerde yüksek kesinlikli noktalar görsel olarak öne çıkar.<br>`375×812`: Dokunulan/seçilen nokta anında `1.00` opaklığa yükselir, alt sayfada tam aralık okunur. |
| **5. İpucu ve Detay** (`CalendarTooltip`, harita hover, `PointDetail.tsx`) | Metin formatı: tam etiketli `PointDetail` satırında `FRP: 45.2 MW (likely 38.7–51.8)` / `FRP: 45,2 MW (olası aralık 38,7–51,8)`; sıkışık ipucunda kısa biçim `45.2 MW (38.7–51.8)` / `45,2 MW (38,7–51,8)`. | `--color-surface-raised`, `--color-text`, `--color-text-muted`, `--color-border` | **Raw:** Yalnızca nokta değer (`FRP: 45.2 MW · Raw observation` / `Ham gözlem`); parantez içinde aralık yazılmaz.<br>**Harmonized:** Alt–üst aralık, VZA çarpanı (`vza_factor`), kalibrasyon oranı (`calibration_ratio`, yalnızca `ModisOnly` ise) ve `> %30` ise geniş aralık notu gösterilir. | Popover/bottom-sheet geçişi `0ms` / `150ms`. | Her iki temada gövde metni `--color-text` (`>= 7:1` kontrast hedefi); birim ve aralık parantezi `--color-text-muted`. | `1366×768`: Sağ yüzer `PointDetail` kartı.<br>`375×812`: Alt sayfa (`bottom sheet`), kaydırmasız ilk ekranda FRP + aralık + kaynak görünür. |
| **6. KPI Kartları & Brifing** (`CountDelta`, `BriefingDrawer.tsx`) | `CountDelta` kartında tespit sayımı değişimi kesin sayım olarak kalır (`±` basılmaz). Brifing özetinde veya ortalama FRP kartında ana değerin altında `11px tabular-nums` alt satır: `Mean FRP: 41.8 MW (±1σ: 35.1–48.5)` + geniş aralık uyarısı. | `--color-text`, `--color-text-muted`, `--color-status-demo` (bilgilendirici not) | **Raw:** Yalnızca ham sayım ve ham ortalama FRP.<br>**Harmonized:** Ortalama FRP aralığı + `UNCERTAINTY_NOTE` özeti eklenir; yazdırılabilir brifingde siyah-beyaz metin olarak korunur. | Sayaç geçişi anlık (`tabular-nums` sıçramasız). | Kart içi metin kontrastı en az AA (`4.5:1`); baskıda `#fff` zemin, `#000` mürekkep. | `1366×768`: Üst şerit veya çekmece başlığı.<br>`375×812`: Alt alta kompakt yığın; taşma olmaması için kısa etiket tercih edilir. |

### 1.1 Harita Noktalarında Neden "Opaklık" Seçildi? (Canvas + WebGL2 + Over-plotting Gerekçesi)

Harita için üç aday değerlendirilmiştir: **opaklık**, **hare (halo) boyutu**, **ikincil halka**.

1. **Hare boyutu neden elendi?**
   - Hem `FlatMap` (`pointRadiusFor` -> `2.2 + (b - 1) * 0.48`) hem `GlobeMap` (`discRadius` + `GLOW_REACH = 2.2`, `GLOW_PEAK = 0.45`) nokta yarıçapını ve ışımasını **FRP şiddetini (MW)** anlatmak için kullanır.
   - Belirsizliği büyük noktalara daha büyük hare vermek, görsel alanı büyüterek kalabalık yangın kümelerinde (*over-plotting*) tam tersi bir yanılsama yaratır: yüksek belirsizliğe sahip bir MODIS kenar-tarama noktası, yüksek kesinlikli VIIRS noktalarından daha "önemli/büyük" görünür.
2. **İkincil halka neden elendi?**
   - Kesikli dış halka halihazırda `map-duplicate-ring` ("Birden çok kez sayıldı / Counted twice") kategorisine atanmıştır. Belirsizlik için ikinci bir halka eklemek hem eş merkezli iki halkanın küçük piksel çaplarında (2–5 px) birbirine karışmasına hem de WebGL fragment shader'ında (`globe/points.ts`) ek alfa/mesafe dallanmasına yol açar.
3. **Neden 3 kademeli nicemlenmiş opaklık?**
   - **Yüksek-kesinlik önceliği:** Yoğun kümelenmelerde belirsizliği yüksek noktaların opaklığının `0.72` veya `0.48`'e düşmesi, arka plandaki kesin (`1.00`) VIIRS/eşleşmiş noktaların görsel ağırlığını korur.
   - **Render maliyetiyle uyum:** `FlatMap` (`engine/renderer.ts`) `DEFAULT_MAX_DRAWN = 30_000` noktayı `Path2D` kovaları halinde tek vuruşta çizer. Sürekli `globalAlpha` değişimi bu gruplamayı bozar; ancak 3 kademe (`bucket 1..8 × 3 kademe = en fazla 24 Path2D`) gruplamayı korur. `GlobeMap` (`globe/pointAttributes.ts` ve `points.ts`) ise tek draw-call `THREE.Points` yapısına yalnızca 1 byte/float `aUncert` niteliği ekleyerek parça shader'ında `alpha *= aUncert` uygulayabilir.
   - **Alt sınır güvencesi:** Opaklık asla `0.48`'in altına düşmez ve zemin kontrastı `uCasing` / `haloLight` ile desteklenir; böylece hiçbir nokta "görünmez" hale gelmez.

### 1.2 Görünüm Taslakları (Wireframes)

**Cliff zaman serisi güven bandı ve BurnCalendar tarama ayrımı (`1366×768` alt panel):**
```text
+-------------------------------------------------------+  +-------------------------------+
| CliffChart (220px yükseklik) · Harmonized · Mean FRP  |  | BurnCalendar · 2012 (Harm.)   |
|  MW ^                                                 |  |          12 (Ara)             |
| 120 |           .---~~~~---.    <- üst sınır (upper)  |  |       .-'---'-.               |
|  80 |     .----'============'---. <- ana seri + bant  |  |  9 .-' ///|    '-. 3          |
|  40 |----'      '---____---'    '-- alt sınır (lower) |  |   |/////|  2012   |  <- ///:   |
|   0 +------+------+------+------+-----> Yıl           |  |    '-.   | (kesik) |    relWidth |
|     2005   2010   2012   2015   2020                  |  |       '-.|________.'     > %30   |
| [ ] Raw (bant yok)   [■] Harm. (±1σ, ~%68 güven)      |  |    6 (Haz)  --- normalArc     |
+-------------------------------------------------------+  +-------------------------------+
```

**PointDetail çekmecesi — Masaüstü (`1366×768`, sağ yüzer) ve Mobil (`375×812`, alt sayfa):**
```text
Masaüstü (genişlik: 312px)                 Mobil (375×812 alt sayfa)
+-----------------------------------+      +------------------------------------+
| [■] Eşleşmiş (MODIS + VIIRS)  [×] |      |  ─── (tutamak)          [×]       |
| 38.50°N, 27.25°E · 2026-07-01 UTC |      | [■] Eşleşmiş · 2026-07-01 10:30 UTC|
+-----------------------------------+      +------------------------------------+
| FRP (armonikleştirilmiş)          |      | FRP: 45,2 MW (olası aralık         |
| 45,2 MW (olası aralık 38,7–51,8)  |      |      38,7–51,8) · ±1σ (~%68)       |
| ±1σ (~%68 güven aralığı)          |      |------------------------------------|
|-----------------------------------|      | Ham FRP: 52,4 MW | VZA: 2.1x       |
| Ham FRP       52,4 MW             |      | [!] Geniş aralık (> %30): sensör   |
| Gözlem sayısı 2 (Aqua, NOAA-20)   |      |     açısı büyük veya kalibrasyon   |
| Bakış açısı   VZA 2.1× (orta)     |      |     verisi az.                     |
|-----------------------------------|      +------------------------------------+
| [!] Geniş aralık: sensör açısı    |
|     büyük veya kalibrasyon verisi |
|     az.                           |
+-----------------------------------+
```

---

## 2. Geniş Aralık Açıklama Metinleri ve ±1σ Güven İfadesi (EN / TR)

### 2.1 Arka Uç Matematiğiyle Birebir Bağlantı

`crates/core/src/uncertainty.rs` ve `calibration.rs` dosyalarındaki mevcut model parametreleri:
- Güven düzeyi `DEFAULT_Z = 1.0`; `confidence_from_z(1.0) = erf(1.0 / sqrt(2)) ≈ 0.6827` -> kullanıcı arayüzünde **`±1σ (~%68)`** olarak ifade edilir.
- Rastgele sensör belirsizliği varsayımları: `DEFAULT_MODIS_SENSOR_SIGMA = 0.12` (%12), `DEFAULT_VIIRS_SENSOR_SIGMA = 0.08` (%8); `n` gözlemde `1/√n` ile azalır.
- Bakış açısı hatası: `DEFAULT_VZA_ERROR_COEFF = 0.05` (`factor > 1.0` iken `0.05 * (factor - 1.0)`; kenar taramada `factor ≈ 4.0` olduğunda `σ_vza = %15`).
- Enlem kalibrasyon belirsizliği (`calibration.rs`, Li et al. 2018): yalnızca `ModisOnly` gözlemlere uygulanır (`Matched` ve `ViirsOnly` VIIRS FRP'sini temel aldığı için `systematic = 0.0`):
  - Tropikler (`0–30°`): `ratio = 1.16`, `ratio_sigma = 0.13`
  - Orta enlemler (`30–50°`): `ratio = 0.97`, `ratio_sigma = 0.03`
  - Yüksek/boreal (`55–70°`): `ratio = 0.82`, `ratio_sigma = 0.18`
- **Geniş aralık tetikleme eşiği:** Göreli toplam genişlik `(upper - lower) / estimate > 0.30` (tek taraflı yarı genişlik `> %15`) olduğunda uyarı gösterilir. Kenar VZA (`~%15`) veya yüksek enlem MODIS (`0.18`) bu eşiği doğrudan tetikler.

### 2.2 EN / TR Arayüz Metinleri ve Yerleşim

Tüm metinler [docs/UX_YAZIM.md](UX_YAZIM.md) kurallarına uygundur (ünlem yok, emoji yok, sakin ve kesin ton).

| Metin Anahtarı (Öneri) | EN Metin | TR Metin | Yerleşim |
| :--- | :--- | :--- | :--- |
| `uncertaintyIntervalInline` | `{val} MW (likely {low}–{high})` | `{val} MW (olası aralık {low}–{high})` | `PointDetail` ana FRP satırı; `CalendarTooltip` ortalama FRP satırı (sayılar `Intl.NumberFormat` ile) |
| `uncertaintyIntervalCompact` | `{val} MW ({low}–{high})` | `{val} MW ({low}–{high})` | Dar harita hover ipucu, tablo hücresi, brifing satırı |
| `uncertaintyConfidenceLabel` | `±1σ (~68% confidence interval)` | `±1σ (~%68 güven aralığı)` | `PointDetail` alt satırı; `MapLegend` opaklık dipnotu; `CliffChart` alt başlığı |
| `uncertaintyWideTitle` | `Wide interval` | `Geniş aralık` | `PointDetail` ve `CalendarTooltip` uyarı rozeti (eşik `> %30`) |
| `uncertaintyWideBody` | `Wide interval = large sensor viewing angle or limited calibration data.` | `Geniş aralık = sensör açısı büyük veya kalibrasyon verisi az.` | `PointDetail` alt not bloğu ve `BurnCalendar` taramalı dilim ipucu |
| `uncertaintyRawNote` | `Raw observation: no harmonization model interval applies.` | `Ham gözlem: uyumlaştırma modeli aralığı uygulanmaz.` | Raw mod açıkken `PointDetail` ve `MapLegend` bilgi satırı |
| `uncertaintyModelCaveat` | `Harmonized values are model estimates, not ground truth. Default sensor noise and viewing-angle weights are modeling assumptions.` | `Harmonize değerler model tahminidir, yer gerçeği değildir. Varsayılan sensör gürültüsü ve bakış açısı ağırlıkları modelleme varsayımlarıdır.` | `MethodsDrawer` "Belirsizlik / Uncertainty" alt bölümü (`UNCERTAINTY_NOTE` karşılığı) + `BriefingDrawer` dipnotu |

**Yerleşim kuralı:** `uncertaintyWideBody` yalnızca göreli genişlik `%30`'u aşan nokta/ay açıldığında görünür (gürültü yaratmaz). `uncertaintyConfidenceLabel` ise sürekli görünür bilgi olarak `MethodsDrawer` ve `MapLegend` altında sabit yer alır.

---

## 3. Renk Skalası Sistemi ve CVD Denetim Kararları

Bu bölümdeki tüm denetim sonuçları [docs/RENK_DENETIMI.md](RENK_DENETIMI.md) raporundan doğrudan alınmıştır (`tools/cvd_check.py`: Machado, Oliveira & Fernandes 2009 şiddet `1.0`; CIEDE2000 Sharma, Wu & Dalal 2005; WCAG 2.x göreli parlaklık).

Mevcut denetim özeti (`tokens.css`): **36 PASS · 5 WARN · 7 FAIL**.

### 3.1 Rol Bazlı Karar Tablosu

| Rol | Mevcut Tokenlar | `docs/RENK_DENETIMI.md` Denetim Sonucu | Karar |
| :--- | :--- | :--- | :--- |
| **Sıralı (FRP — varsayılan `inferno`)** | `--frp-1..8` (`dark`: `#3d0f5e` -> `#f6e27a`; `light`: `#fbe7a1` -> `#3d0f5e`) | `L*` tekdüze ve ardışık `ΔE00 >= 5` **PASS** (min: `5.5` protan 1–2 dark; `5.2` akrom 1–2 ve `6.2` protan 7–8 light).<br>**FAIL (vs `map-land`):** Dark'ta min `1.20` (basamak 1–3 < 3:1), Light'ta min `1.13` (basamak 1–4 < 3:1). Sıcak uçlar `13.47` (dark) / `13.46` (light). | **Varsayılanı koru + zorunlu kılıf (`haloLight`/`uCasing`):** Düşük FRP ucunun zeminle yakın kalması yoğun sıcak noktaları bastırır; kılıf konturu görünürlüğü garanti eder. Ek olarak K4'te **Viridis** ve **Cividis** seçenekleri sunulur. |
| **Sapma (Anomali — PuOr)** | `--anom-neg-3..1`, `--anom-0`, `--anom-pos-1..3`, `--anom-none` | Kollar tekdüze (`dark` neg `73,56,41`, pos `82,66,49`; `light` neg `80,52,27`, pos `84,63,43`). `neg-k vs pos-k` normal/protan/deutan/tritan tümü `>= 22.2` **PASS**.<br>**FAIL:** Light `anom-0 / anom-none` `ΔE00 = 2.9` (`#eceef2` vs `#f9fafb`).<br>**WARN:** Dark `anom-0 / anom-none` `ΔE00 = 7.7`. Akromatopsi `neg-1 / pos-1` light `3.2`, dark `6.0`. | **Kolları koru, `anom-0` ve `anom-none` ayrımını düzelt:** `anom-none` dolgu rengi yerine şeffaf/çok düşük zemin + `stroke-dasharray: 2 2` çerçeve ve nötr `--anom-0` parlaklık farkıyla ayrıştırılır (yeni hex **denetimden geçirilmeli**). |
| **Kategorik — UI Sensörleri** | `--color-sensor-modis`, `--color-sensor-viirs`, `--color-duplicate-ring` | MODIS vs VIIRS **PASS** (`18.1–18.8` dark, `30.5–31.1` light).<br>**WARN:** Dark `color-sensor-modis / color-duplicate-ring` (`ΔE00 = 6.2` normal, `5.7` akrom). | **Şekil birincil + dark `duplicate-ring` ayrıştır:** Renk çiftini desteklemek için kesikli halka dokusu korunur ve `--color-duplicate-ring` tonu ayrıştırılır (**denetimden geçirilmeli**). |
| **Kategorik — Harita Sensörleri** | `--map-modis`, `--map-viirs`, `--map-duplicate-ring` | MODIS vs VIIRS **PASS** (`18.1–18.8` dark, `21.3–21.7` light).<br>**FAIL:** Dark `map-modis / map-duplicate-ring` `ΔE00 = 3.5` normal, `3.5` protan, `3.9` deutan, `3.6` tritan, `2.5` akrom (`#e2e7f0` vs `#f1f3f7`). | **Değiştir (`map-duplicate-ring`):** Koyu temada `--map-duplicate-ring` sıcak kemik/kum veya açık camgöbeği tona çekilir (**denetimden geçirilmeli**). Kesikli geometri zorunludur. |
| **Kategorik — Durum Rozetleri** | `--color-status-live`, `--color-status-demo`, `--color-status-error` | Zemin/kart kontrastı **PASS** (`>= 4.57:1` light, `>= 8.90:1` dark).<br>**FAIL (Deutan):** Dark `live / error` `4.7`, `demo / error` `3.3`; Light `demo / error` `3.8`.<br>**WARN:** Dark `live / demo` (`6.8–7.4`), Light `live / demo` (`8.5–11.3`), `live / error` (`8.4–10.5`). | **Tonları ayrıştır + zorunlu ikon/metin:** `live` mavi-yeşil/camgöbeği (`●`), `demo` kehribar (`◆`), `error` koyu/parlak kırmızı-macenta (`▲`) olarak ayrıştırılır (**denetimden geçirilmeli**) ve rozet asla metinsiz kullanılmaz. |

### 3.2 Sıralı FRP Palet Seçenekleri — Dark ve Light Hex Tabloları

`viridis` ve `cividis` dizileri [docs/RENK_DENETIMI.md](RENK_DENETIMI.md) içinde `tools/cvd_check.py` tarafından denetlenmiş resmi adaylardır. Light temada koyu noktaların sıcak yangını vurgulaması için rampa **ters yönde** (`1 = açık/düşük -> 8 = koyu/yüksek`) eşlenir.

| Basamak | Varsayılan `inferno` (Dark) | Varsayılan `inferno` (Light) | `viridis` (Dark) | `viridis` (Light, ters) | `cividis` (Dark) | `cividis` (Light, ters) |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `1` (düşük) | `#3d0f5e` | `#fbe7a1` | `#440154` | `#fde725` | `#00204d` | `#ffea46` |
| `2` | `#6a176e` | `#fcc85a` | `#46327e` | `#9fda3a` | `#10396e` | `#dac561` |
| `3` | `#932667` | `#f89b2c` | `#365c8d` | `#4ac16d` | `#4a536c` | `#b3a771` |
| `4` | `#bc3754` | `#e8682a` | `#277f8e` | `#1fa187` | `#6c6e72` | `#8e8a78` |
| `5` | `#dd513a` | `#c8413f` | `#1fa187` | `#277f8e` | `#8e8a78` | `#6c6e72` |
| `6` | `#f37819` | `#9c2a5c` | `#4ac16d` | `#365c8d` | `#b3a771` | `#4a536c` |
| `7` | `#fcb519` | `#6e1c6c` | `#9fda3a` | `#46327e` | `#dac561` | `#10396e` |
| `8` (yüksek) | `#f6e27a` | `#3d0f5e` | `#fde725` | `#440154` | `#ffea46` | `#00204d` |

**Denetlenen `viridis` ve `cividis` metrikleri ([docs/RENK_DENETIMI.md](RENK_DENETIMI.md)):**
- **Viridis:** `L*` tekdüze (`15..91`), ardışık `ΔE00 >= 5` **PASS** (min `5.3` protan 7–8, `6.7` akrom, `8.0` tritan, `8.5` deutan, `12.2` normal). `map-land` karşı sıcak uç kontrastı `13.93` (dark) / `13.95` (light); yalnızca düşük `1–3` basamakları `3:1` altındadır (`1=1.15`, `2=1.69`, `3=2.57`).
- **Cividis:** `L*` tekdüze (`13..92`), ardışık `ΔE00 >= 5` **PASS** (min `7.7` akromatopsi, `7.9` tritan, `8.2` protan, `8.3` deutan, `8.4` normal). `map-land` karşı sıcak uç kontrastı `14.33` (dark) / `14.64` (light); düşük `1–3` basamakları `1.10`, `1.53`, `2.30`. **Akromatopsi ve şiddetli CVD için en tutarlı palettir.**

### 3.3 Yeni Token Önerileri (ve Düzeltme Adayları)

Aşağıdaki tablo, [web/src/styles/tokens.css](../web/src/styles/tokens.css) dosyasında henüz bulunmayan önerileri listeler. Uygulamada `viridis` ve `cividis` hem doğrudan adlandırılmış `--frp-cb-*` tokenları olarak hem de `[data-palette="viridis"]` / `[data-palette="cividis"]` seçicisi altında `--frp-1..8` üstüne yazılarak harita/efsane uyumluluğu sağlanır.

| Token Adı (Öneri) | Rol | Önerilen Değer (Dark) | Önerilen Değer (Light) | Denetim Durumu |
| :--- | :--- | :--- | :--- | :--- |
| `--frp-cb-viridis-1..8` | Sıralı FRP (`viridis`) | Tablo 3.2 (`#440154` -> `#fde725`) | Tablo 3.2 (`#fde725` -> `#440154`) | **Denetlendi (PASS `ΔE00/L*`; düşük 1–3 zemin kontrastı hariç)** |
| `--frp-cb-cividis-1..8` | Sıralı FRP (`cividis`) | Tablo 3.2 (`#00204d` -> `#ffea46`) | Tablo 3.2 (`#ffea46` -> `#00204d`) | **Denetlendi (PASS `ΔE00/L*`; düşük 1–3 zemin kontrastı hariç)** |
| `--color-uncertainty-band` | `CliffChart` / VZA güven bandı dolgu | `rgb(148 156 171 / 0.22)` | `rgb(82 91 107 / 0.16)` | UI katmanı (ana çizgiyle kontrastı korunur; `TASARIM_METHODS.md` ile ortak) |
| `--color-uncertainty-band-stroke` | `CliffChart` üst/alt kesikli sınır | `rgba(203, 213, 225, 0.45)` | `rgba(71, 85, 105, 0.40)` | UI katmanı |
| `--uncert-errorbar-stroke` | Çubuk/histogram hata çubuğu | `#e2e8f0` (koyu zemin/çubuk) | `#1e293b` (açık zemin/çubuk) | **denetimden geçirilmeli** |
| `--uncert-hatch-stroke` | `BurnCalendar` `> %30` tarama çizgisi | `rgba(248, 250, 252, 0.28)` | `rgba(15, 23, 42, 0.26)` | SVG desen katmanı (alttaki PuOr doldurmayı korur) |
| `--uncert-slice-border` | `BurnCalendar` `> %30` kesikli kenar | `#cbd5e1` | `#475569` | **denetimden geçirilmeli** |
| `--uncert-alpha-tight` / `-mid` / `-wide` | Harita nokta opaklık kademeleri | `1.0` / `0.72` / `0.48` | `1.0` / `0.72` / `0.48` | Sayısal çarpan (renk üretmez) |
| `--map-duplicate-ring` (düzeltme önerisi) | Çift sayım halkası FAIL çözümü | `#f5d0a9` (sıcak şeftali-kemik; `--map-modis` `#e2e7f0` ile ayrışır) | `#12161d` (mevcut light değer korunur; bkz. 3.4) | **denetimden geçirilmeli** (dark öneri) |
| `--color-duplicate-ring` (düzeltme önerisi) | UI çift sayım halkası WARN çözümü | `#f5d0a9` | `#12161d` (mevcut light değer korunur; bkz. 3.4) | **denetimden geçirilmeli** (dark öneri) |
| `--anom-0` / `--anom-none` (düzeltme önerisi) | Nötr vs veri yok ayrımı (Light FAIL, Dark WARN) | `--anom-0: #333b4d` · `--anom-none: #161a24` | `--anom-0: #dce1ea` · `--anom-none: #f9fafb` | **denetimden geçirilmeli** |
| `--color-status-live` (düzeltme önerisi) | Canlı veri durumu (Deutan FAIL çözümü) | `#2dd4bf` (camgöbeği-yeşil) | `#0f766e` | **denetimden geçirilmeli** |
| `--color-status-demo` (düzeltme önerisi) | Örnek/demo durumu (Deutan FAIL çözümü) | `#fbbf24` (altın-kehribar) | `#b45309` | **denetimden geçirilmeli** |
| `--color-status-error` (düzeltme önerisi) | Hata durumu (Deutan FAIL çözümü) | `#fb7185` (parlak gül-kırmızı) | `#be123c` | **denetimden geçirilmeli** |

### 3.4 Düzeltme Adaylarının Simülatör Denetimi (2026-11-15)

Yukarıdaki "denetimden geçirilmeli" değerleri, `tools/cvd_check.py` içindeki aynı fonksiyonlarla (Machado 2009 protan/deutan/tritan, CIEDE2000) `tokens.css` değiştirilmeden, bellekte yamanmış token setiyle çalıştırıldı. Sayılar, normal görüş ve üç simülasyon arasındaki **en küçük** ΔE00 değeridir.

| Kontrol | Mevcut | Öneriyle | Sonuç / Karar |
| :--- | :--- | :--- | :--- |
| Dark `map-modis` / `map-duplicate-ring` (`#f5d0a9`) | 3.5 FAIL | 21.0 | **Kabul.** Not: `#f5d0a9` ile `--frp-8` arası 5.9; halka yalnız Ham modda, FRP skalası Harmonize modda çizildiği için çakışmaz. |
| Dark `color-sensor-modis` / `color-duplicate-ring` (`#f5d0a9`) | 6.2 WARN | 20.6 | **Kabul.** |
| Light `map-duplicate-ring` / `color-duplicate-ring` | `#12161d` PASS (10.5 / 14.4) | `#475569` → 6.6 WARN | **Reddedildi.** Mevcut light değer `#12161d` korunur. |
| Dark durum `live` / `demo` / `error` (`#2dd4bf` / `#fbbf24` / `#fb7185`) | 2 FAIL + 1 WARN | tüm çiftler PASS | **Kabul.** |
| Light durum `demo` / `error` (`#b45309` / `#be123c`) | 3.8 FAIL | deutan 9.3, tritan 6.8 → WARN | **`--color-status-error` light için `#9f1239` kullanılsın:** deutan 16.2, tritan 9.6 (eşik sınırında). Sembol eşlemesi (`● / ◆ / ▲`) zorunlu kalır. |
| Light `anom-0` / `anom-none` (`#dce1ea` / `#f9fafb`) | 2.9 FAIL | 6.5 WARN | **Kabul + kesikli kenar.** Daha koyu nötr (`#c8cfdb` → 10.8) `--anom-neg-1` ile ayrımı 5.4'e düşürdüğü için seçilmedi; "veri yok" hücresi renge ek olarak `2 2` kesikli kenarla ayrılır (bkz. 4.2). |
| Dark `anom-0` / `anom-none` (`#333b4d` / `#161a24`) | 7.7 WARN | PASS | **Kabul.** |
| Sıralı FRP adım 1–3 (dark) / 1–4 (light) ile `--map-land` kontrastı | < 3:1 FAIL | değişmez | **Bilinçli kabul.** Sıralı rampanın düşük ucu zemine yaklaşır; viridis/cividis'te de aynı (1.1:1). Çözüm renk değil, nokta halesi (`--map-halo-light`) ve 1.1'deki opaklık kuralıdır. |

Toplam: sayılan FAIL sayısı **7 → 2** (kalan ikisi yukarıdaki bilinçli FRP–zemin kabulü), WARN sayısı **5 → 2** (light `demo`/`error` tritan 9.6 ve light `anom-0`/`anom-none` 6.5; ikisi de şekil/kenar ikinci kodlamasıyla destekleniyor). Light `#9f1239` ile `live`/`error` 17.4, `live`/`demo` 23.4 kalır; `--color-bg` ve `--color-surface` karşısında kontrast 7.5:1 ve 8.0:1'dir.

> [!IMPORTANT]
> K4 bu değerleri `tokens.css`'e işledikten hemen sonra `python tools/cvd_check.py --out docs/RENK_DENETIMI.md` komutunu çalıştırıp raporu yeniden üretmelidir. Rapor, depodaki gerçek token'ları ölçer; bu tablo yalnızca öneri aşamasının kaydıdır.

---

## 4. Sensör Kodlaması (MODIS / VIIRS / Harmonized) ve Anomali Skalası Anlamı

### 4.1 Şekil-Öncelikli Sensör ve Katman Kodlaması ([docs/TASARIM_SISTEMI.md](TASARIM_SISTEMI.md))

Renk tek başına sensör ayrımı için kullanılmaz; monokrom baskıda ve ağır CVD altında biçim farkı korunur:

| Varlık / Katman | Şekil ve Geometri | Renk Tokenı | Ek İşaret / Not |
| :--- | :--- | :--- | :--- |
| **MODIS (Raw katmanı)** | **İçi boş daire** (`PointShape.HollowCircle`, `1.5px` kontur) | UI: `--color-sensor-modis`<br>Harita: `--map-modis` | `CliffChart` üzerinde noktalı çizgi (`2 2`) + açık içi boş daire düğümleri. |
| **VIIRS (Raw katmanı)** | **Dolu kare** (`PointShape.Square`) | UI: `--color-sensor-viirs`<br>Harita: `--map-viirs` | `CliffChart` üzerinde dolu kare işaretçiler; yüksek uzamsal çözünürlük yoğunluğunu gösterir. |
| **Harmonized (birleşik nokta)** | **Dolu disk + ince zemin kılıfı** (`PointShape.Circle` + `haloLight` / `uCasing`) | Dolgu: `--frp-1..8` (veya seçili palet) | Boyut FRP kovasına (`bucket 1..8`) bağlıdır; opaklık belirsizlik kademesine (`1.0 / 0.72 / 0.48`) bağlıdır. |
| **Çift sayım / Eşleşen nokta** | **Kesikli dış halka** (`dash [2, 2]` / `aDup`) | UI: `--color-duplicate-ring`<br>Harita: `--map-duplicate-ring` | "Birden çok kez sayıldı" uyarısını taşır; dolgu renginden bağımsız geometriyle seçilir. |
| **Durum rozetleri (Header / Drawer)** | **Sabit sembol + metin:** `●` Canlı, `◆` Demo, `▲` Hata | `--color-status-live`, `--color-status-demo`, `--color-status-error` | Sembol ve metin asla ayrı bırakılmaz; CVD altında dahi karıştırılmaz. |

### 4.2 Anomali Skalası Anlamı (`BurnCalendar` PuOr)

`scales.ts` içindeki `anomalyBucket(z)` yedi ayrık seviye ve bir boş veri durumu üretir:

- **Mor (`--anom-neg-3`, `--anom-neg-2`, `--anom-neg-1`) -> Normalin altı:** `z <= -0.5` (`-1`, `-2`, `-3` şiddet basamakları). Tarihsel ortalama yangın aktivitesinin altında geçen aylar.
- **Nötr Gri (`--anom-0`) -> Normal aralık:** `-0.5 < z < +0.5`. Uzun dönem mevsim normallerine yakın aktivite.
- **Turuncu (`--anom-pos-1`, `--anom-pos-2`, `--anom-pos-3`) -> Normalin üstü:** `z >= +0.5` (`+1`, `+2`, `+3` şiddet basamakları). Tarihsel normalin belirgin biçimde üzerindeki aylar.
- **Veri yok (`--anom-none`) -> Kesikli çerçeve (`stroke-dasharray: 2 2`):** İlgili ayda gözlem/arşiv kaydı yok. `BurnCalendar` üzerinde nötr `0` ile karıştırılmaması için hem belirgin açık/koyu zemin farkı (`3.3` önerisi) hem de kesikli kenar ve `"Veri yok"` ipucu zorunludur.
- **Akromatopsi uyarısı:** [docs/RENK_DENETIMI.md](RENK_DENETIMI.md) ölçümüne göre gri tonlamada `anom-neg-1` ile `anom-pos-1` birbirine yakındır (`ΔE00 = 3.2` light, `6.0` dark). Bu nedenle `CalendarTooltip` ve erişilebilirlik etiketleri (`aria-label`) her ay için açık sayısal işareti (`-0.8σ` / `+1.4σ`, `"normalin altı"` / `"normalin üstü"`) her zaman yazar.

---

## 5. Gruplama Yöntemi (Quantile vs Sabit Aralık) ve Lejant Wireframe'i

### 5.1 Görünüm Bazlı Gruplama Kararı

Mevcut kodda (`web/src/lib/scales.ts`) `FRP_BREAKS = [5, 10, 20, 40, 80, 160, 320]` sabit logaritmik eşikler kullanılır. K4 için görünüm türüne göre gruplama stratejisi ayrıştırılır:

| Görünüm | Seçilen Gruplama Yöntemi | Gerekçe ve Aykırı Değer (Outlier) Yönetimi | Lejant Eşik Gösterimi |
| :--- | :--- | :--- | :--- |
| **Harita FRP** (`FlatMap` & `GlobeMap`) | **Varsayılan: Görünüm/bölge içi 8 dilimli Quantile (sekizdelik).**<br>**Yedek (Fallback):** Nokta sayısı `< 64` ise veya quantile sınırları çakışırsa sabit log `FRP_BREAKS` `[5, 10, 20, 40, 80, 160, 320]`. | Yangın FRP dağılımı aşırı sağa çarpıktır: pek çok `5–40 MW` nokta varken nadir megayangınlar `1000+ MW` değerine ulaşır. Eşit aralık (`linear`) tüm veriyi ilk 1–2 basamağa hapseder; sabit log ise soğuk mevsimlerle büyük olayları eşit ayırmaz. Quantile her zaman 8 renk basamağını dengeli kullanır. **Aykırı değer kuralı:** En üst basamak (`8. dilim`, `P87.5–Maks`) üst sınırda kırpılmaz; efsane dipnotunda `P99` veya `Maks` değeri (`ör. üst eşik P87.5 = 94 MW · maks 612 MW`) belirtilir. | 8 renk kutucuğunun altında hesaplanan 7 ara eşik (`Q1..Q7`) yuvarlanmış tamsayı olarak gösterilir (ör. `4, 9, 16, 28, 47, 79, 145 MW+`). Quantile veya Sabit mod küçük bir alt etiketle belirtilir (`Quantile · Bölge` / `Fixed log`). |
| **Takvim Anomalisi** (`BurnCalendar`) | **Sabit, sıfır merkezli z-skoru eşikleri:** `[-2, -1, -0.5, +0.5, +1, +2]` (`anomalyBucket`). | Anomali ekseninin fiziksel sıfır noktası ("tarihsel ortalama") vardır. Quantile kullanılırsa yangını az bir yılın ortalaması yapay olarak turuncuya boyanır; farklı yıllar ve bölgeler karşılaştırılamaz. Sabit `σ` eşikleri korunur. | Yıl saati dipnotunda renk çifti anlamı (`Mor < -0.5σ · Turuncu > +0.5σ`) + tarama eşiği (`Tarama = aralık > %30`). |
| **Cliff Zaman Serisi** (`CliffChart`) | **Sürekli doğrusal eksen** (gruplama yok). | Yıllık eğri ve `2012` kırılma büyüklüğü oranları bozulmamalıdır. | Eksen etiketleri + Raw / Harmonized bant açıklaması. |

### 5.2 `MapLegend` Bileşeni Wireframe (Masaüstü & Mobil)

Mevcut `MapLegend.tsx` yapısı üzerine belirsizlik satırı, `"Veri yok"` kutucuğu ve dinamik quantile eşik göstergesi eklenir.

**Masaüstü (`1366×768`, sol alt yüzer cam/düz kart, max genişlik ~276px):**
```text
+---------------------------------------------------+
| FRP ŞİDDETİ (MW) · Quantile (bölge)    [Viridis]  |  <- Palet rozeti (opsiyonel kısayol)
| +----+----+----+----+----+----+----+----+  +----+ |
| | 1  | 2  | 3  | 4  | 5  | 6  | 7  | 8  |  |////| |  <- [////] = "Veri yok / Yok"
| +----+----+----+----+----+----+----+----+  +----+ |
|     4    9   16   28   47   79  145+    Veri yok  |  <- 7 ara eşik + maks ipucu
|   (maks: 612 MW · P99: 340 MW)                    |
|---------------------------------------------------|
| (o) MODIS  [■] VIIRS  (:) Çift sayım              |  <- Şekil-öncelikli sensörler
| Opaklık: dolu = kesin (<=%20) · soluk = geniş (>%40)
| ±1σ (~%68 güven) · Aralık detayı için nokta seçin  |
+---------------------------------------------------+
```

**Mobil (`375×812`, harita alt solunda dar veya katlanabilir şerit):**
```text
Katlı durum (varsayılan, map alanını kapatmaz):
+------------------------------------+
| [■■■□□] FRP (MW) · Viridis   [Aç v]|
+------------------------------------+

Açık durum (dokunulduğunda yukarı genişler):
+------------------------------------+
| FRP (MW) · Quantile       [Kapat ^]|
| [1][2][3][4][5][6][7][8]  [ ] Yok  |
|   4    16   47   145+ (maks 612)   |
| (o) MODIS [■] VIIRS (:) Çift sayım |
| Soluk nokta = geniş aralık (±1σ)   |
+------------------------------------+
```

---

## 6. Palet Seçici UX (K4 Tasarımı)

### 6.1 Yerleşim, Etiketler ve Kalıcılık

- **Birincil yerleşim:** `MethodsDrawer.tsx` içindeki mevcut **`Görünüm` / `Display`** ayar bölümü (Tema `Koyu`/`Açık` ve `Sunum modu` kontrollerinin hemen altı). Bilimsel bir araçta palet seçimi birincil analiz modu değil erişilebilirlik/tercih ayarıdır; üst çubuğu (`Header.tsx`) kalabalıklaştırmamak için `MethodsDrawer` ana yuva olarak seçilmiştir.
- **İkincil kısayol:** `MapLegend` sağ üst köşesinde aktif palet adı (`Inferno` / `Viridis` / `Cividis`) tıklanabilir küçük bir kapsül olarak görünür; tıklandığında `MethodsDrawer` görünüm bölümüne odaklanır (veya doğrudan 3 seçenekli kompakt menüyü açar).

| Seçenek ID | EN Etiket (`copy.ts` önerisi) | TR Etiket (`copy.ts` önerisi) | Kısa Açıklama (EN / TR) |
| :--- | :--- | :--- | :--- |
| `default` | `Inferno (Default)` | `Inferno (Varsayılan)` | `High-contrast thermal ramp` / `Yüksek kontrastlı ısıl rampa` |
| `viridis` | `Viridis (Colorblind-safe)` | `Viridis (Renk Körlüğü Uyumlu)` | `Perceptually uniform purple-to-yellow` / `Algısal olarak eşit mor–sarı rampa` |
| `cividis` | `Cividis (High CVD-safe)` | `Cividis (Yüksek CVD Uyumlu)` | `Optimized for red-green & achromatopsia` / `Kırmızı-yeşil ve akromatopsi için optimize` |

- **Kalıcılık (Persistence):**
  1. `localStorage` anahtarı: `pyrosync.palette` (`"default" | "viridis" | "cividis"`), mevcut `pyrosync.theme` anahtarıyla simetrik (`useAppStore`).
  2. URL hash (`web/src/lib/urlState.ts`): `#region=...&mode=...&month=...&lang=...&palette=viridis`. Varsayılan değer (`"default"`) URL'de yazılmaz (mevcut `region`/`lang` kuralıyla aynı); yalnızca `viridis` veya `cividis` seçiliyse paylaşım linkine eklenir.

### 6.2 Tüm Grafiklerde Eşzamanlı Güncelleme ve `@media print`

1. `useAppStore` içindeki `palette` değiştiğinde `useDocumentSync` `document.documentElement.dataset.palette` özniteliğini günceller (`"default"` iken kaldırılır veya `"default"` atanır).
2. `web/src/styles/tokens.css` `[data-palette="viridis"]` ve `[data-palette="cividis"]` seçicileriyle (hem koyu hem `:root[data-theme="light"]` için) `--frp-1..8` değişkenlerini anında değiştirir.
3. **CSS/SVG bileşenleri** (`MapLegend`, `BurnCalendar`, `CliffChart`, `MethodsDrawer`): CSS değişkenlerini doğrudan okudukları için tarayıcı katmanında sıfır JS maliyetiyle güncellenir.
4. **Canvas ve WebGL2 harita** (`FlatMap` & `GlobeMap`): Bugün `usePaletteVersion(viewportRef)` (`web/src/components/Map/useMapModel.ts`) `attributeFilter` listesinde yalnızca `["data-theme", "class", "style"]` özniteliklerini izler ve `buildPaletteSignature` (`web/src/components/Map/engine/renderer.ts`) yalnızca `dataset.theme` değerini imzaya katar. Bu nedenle K4 aşamasında hem `useMapModel.ts` içindeki `attributeFilter` dizisine `"data-palette"` hem de `renderer.ts` içindeki `buildPaletteSignature` fonksiyonuna `dataset.palette` eklenmelidir; böylece `paletteVersion` artar, `readPalette()` önbelleği yenilenir ve `GlobeMap` `uFrp[8]` uniform dizisini (`globe/points.ts`) tek karede günceller.
5. **`@media print` davranışı:** `web/src/styles/global.css` ve `BriefingDrawer.module.css` içindeki baskı kuralı (`background: #fff; color: #000`) geçerlidir. Yazdırma sırasında seçili palet **korunur** ancak `data-theme="light"` kontrast değerleri (açık zeminde koyu sıcak uçlar) zorlanır; grafikler renksiz yazıcıya düşse dahi MODIS boş daire, VIIRS dolu kare, çift sayım kesikli halkası ve `BurnCalendar` `> %30` tarama deseni bilgi kaybını önler.

---

## 7. Light / Dark Kontrast Kuralları ve Akromatopsi / Baskı Denetimi

[docs/RENK_DENETIMI.md](RENK_DENETIMI.md) raporundaki sayısal bulgulara dayanan zorunlu arayüz kuralları:

1. **UI Metin ve Vurgu Kontrastı (PASS):**
   - Dark temada tüm vurgu ve durum renkleri `--color-bg` (`#0d0f13`) ve `--color-surface` (`#14171d`) üzerinde `>= 8.90:1` kontrast verir (`--color-accent` `#3cc8f0`: `9.78` / `9.16`, `--color-status-error` `#e3a99b`: `9.51` / `8.90`).
   - Light temada (`--color-bg` `#f6f7f9`, `--color-surface` `#ffffff`) tüm vurgu ve durum renkleri `>= 4.57:1` kontrast verir (`--color-accent` `#0a6f99`: `5.23` / `5.60`, `--color-status-live` `#2f7d4a`: `4.72` / `5.06`, minimum `--color-status-demo` `#8a6d1f`: `4.57` / `4.90`). Yeni veya düzeltilmiş her token Light temada `>= 4.5:1` (metin) ve `>= 3.0:1` (grafik işareti) sınırını korumalıdır.
2. **Harita Zemin (`map-land`) Kontrastı ve Zorunlu Kılıf Kuralı:**
   - Mevcut denetimde tüm 3 sıralı palet de yalnızca en düşük FRP basamaklarında `map-land` ile `3:1` altındadır (`inferno` dark `1.20–2.27`, light `1.13–2.98`; `viridis` dark `1.15–2.57`; `cividis` dark `1.10–2.30`). En yüksek sıcaklık basamağı ise `13.46–14.64:1` kontrast verir.
   - **Kural:** Düşük FRP noktalarının zeminle tamamen yutulmaması için `FlatMap` (`haloLight`) ve `GlobeMap` (`uCasing`) üzerinde zeminle zıt ince kılıf (`1px` açık/koyu kontur) **her zaman açık** tutulur; asla kullanıcıya kapatma seçeneği sunulmaz.
3. **Akromatopsi (Tam Renk Körlüğü) ve Gri Tonlama:**
   - Sıralı FRP için **en güvenilir tek-ton (luminance) ayrımı `cividis`'tedir** (akromatopsida en küçük ardışık adım `ΔE00 = 7.7`; `viridis` `6.7`; `inferno` light `5.2`).
   - Sapma skalasında (`PuOr`) normal/protan/deutan/tritan `>= 22.2` ayrım sağlasa da akromatopsida `anom-neg-1 / anom-pos-1` çifti `6.0` (dark) ve `3.2` (light) değerine düşer. Bu yüzden `BurnCalendar` dilimleri üzerine tıklandığında/odaklandığında okunan `z` işareti (`-0.8σ` / `+1.4σ`) ve `aria-label` metni ikincil kanal değil zorunlu eşlikçidir.
4. **Durum Rozetleri ve Çift Sayım Halkası:**
   - Deutan simülasyonunda mevcut `status-demo / status-error` (`3.3` dark, `3.8` light) ve `status-live / status-error` (`4.7` dark) FAIL durumundadır; bu nedenle Bölüm 3.3'teki ton ayrımı + `● / ◆ / ▲` ikonları K4 tamamlanana kadar tek güvenli bariyerdir.

---

## 8. Devir Listeleri (K3 & K4) ve "Bitti sayılır" Kontrol Listesi

### 8.1 K3 Devir Listesi (Belirsizlik Bağlama & Sayısal İfade)

1. **Veri sözleşmesi ve tipler (`web/src/types/index.ts`, `crates/server/src/main.rs`):**
   - `HarmonizedPoint` tipine `frpLower`, `frpUpper`, `vzaFactor?`, `calibrationRatio?` alanlarını ekle (`/api/points` zaten `frp_lower`/`frp_upper` gönderiyor).
   - `CalendarCell` ve `/api/series` (`MonthEntry`/`YearEntry`) tarafında `meanFrp`, `meanFrpLower`, `meanFrpUpper` alanlarını taşı.
2. **Ortak yardımcı ve formatlayıcı (`web/src/lib/format.ts`):**
   - Göreli genişlik hesabı `relativeUncertaintyWidth(lower, upper, estimate)` ve geniş eşik sabiti (`WIDE_UNCERTAINTY_THRESHOLD = 0.30`).
   - `formatFrpInterval(val, lower, upper, lang, compact?)` (`\u00A0MW`, `Intl.NumberFormat`).
3. **Bileşen uygulamaları:**
   - `CliffChart.tsx`: Harmonized modda güven bandı `<path>` (animasyon `prefers-reduced-motion` ile kısaltılmış/kapalı).
   - `BurnCalendar.tsx` & `CalendarTooltip.tsx`: `<defs><pattern id="uncert-hatch">` taraması + `relWidth > 0.30` ise kesikli kenarlık + `uncertaintyWideBody` ipucu satırı.
   - `FlatMap.tsx` (`engine/renderer.ts`): `bucket × 3` kademeli opaklık (`1.0 / 0.72 / 0.48`) ile `Path2D` çizimi; seçili noktada `1.0`.
   - `GlobeMap.tsx` (`globe/pointAttributes.ts` & `globe/points.ts`): `aUncert` float attribute'u + fragment shader alpha çarpanı.
   - `PointDetail.tsx`, `BriefingDrawer.tsx`, `MethodsDrawer.tsx`: Bölüm 2.2 EN/TR metinleri (`copy.ts`), `UNCERTAINTY_NOTE` ve `±1σ (~%68)` etiketi.

### 8.2 K4 Devir Listesi (Renk Skalası, Quantile ve Palet Seçici)

1. **Tokenlar ve renk denetimi (`web/src/styles/tokens.css`):**
   - `[data-palette="viridis"]` ve `[data-palette="cividis"]` altında Tablo 3.2 hexlerini hem koyu hem `:root[data-theme="light"]` için ekle; `--frp-cb-*` takma adlarını ve Bölüm 3.3 düzeltme hexlerini (`--map-duplicate-ring`, `--anom-0`, `--anom-none`, `--color-status-*`) tanımla.
   - **Zorunlu doğrulama:** Değişiklik sonrası `python tools/cvd_check.py --out docs/RENK_DENETIMI.md` çalıştır ve tüm `FAIL` (ve mümkün tüm `WARN`) kayıtlarının `PASS`'a geçtiğini teyit et.
2. **Durum, URL ve senkronizasyon (`useAppStore`, `web/src/lib/urlState.ts`, `useDocumentSync`):**
   - `palette: 'default' | 'viridis' | 'cividis'` durumunu, `pyrosync.palette` `localStorage` anahtarını ve `#palette=` URL hash okuma/yazmasını ekle.
   - `useDocumentSync` ile `document.documentElement.dataset.palette` eşle; ayrıca `useMapModel.ts` `attributeFilter` dizisine `"data-palette"` ve `renderer.ts` `buildPaletteSignature` imzasına `dataset.palette` ekle.
3. **Quantile gruplama ve UI (`web/src/lib/scales.ts`, `MapLegend.tsx`, `MethodsDrawer.tsx`):**
   - `computeQuantileBreaks(values, buckets = 8)` (`values.length < 64` veya dejenere aralıkta `FRP_BREAKS` fallback) + `P99`/`Maks` üst sınırı hesabı.
   - `MapLegend.tsx` için Bölüm 5.2 masaüstü/mobil wireframe'ini (`Veri yok` kutucuğu, opaklık satırı, dinamik eşik tikleri, opsiyonel palet kapsülü) uygula.
   - `MethodsDrawer.tsx` "Görünüm / Display" bölümüne 3 seçenekli palet seçiciyi ve `● / ◆ / ▲` durum ikonlarını ekle.

### 8.3 C7 "Bitti sayılır" Kontrol Listesi

| Gereksinim | Kapsanan Bölüm | Doğrulama / Referans |
| :--- | :--- | :--- |
| Grafik türüne göre belirsizlik karar tablosu (Cliff, çubuk, takvim, harita, tooltip/`PointDetail`, KPI) + token, Raw/Harm, `reduced-motion`, Light/Dark | Bölüm 1, 1.1, 1.2 | Mevcut `CliffChart`, `BurnCalendar`, `FlatMap`, `GlobeMap`, `CountDelta` yapılarıyla hizalı |
| Geniş aralık açıklaması (EN/TR) + `±1σ (~%68)` güven ifadesi ve kod sabitleri (`DEFAULT_Z = 1.0`, VZA, Li et al. 2018) | Bölüm 2, 2.1, 2.2 | `crates/core/src/uncertainty.rs` ve `calibration.rs` ile doğrulandı |
| Sıralı, sapma ve kategorik roller + [docs/RENK_DENETIMI.md](RENK_DENETIMI.md) sonuçlarıyla kararlar + `viridis`/`cividis` hex tabloları | Bölüm 3, 3.1, 3.2 | `7 FAIL, 5 WARN, 36 PASS` rakamları birebir alıntılandı |
| "Yeni token önerileri" tablosu ve denetlenmemiş hexlerin `"denetimden geçirilmeli"` işareti | Bölüm 3.3 | Yeniden denetim komutu: `python tools/cvd_check.py --out docs/RENK_DENETIMI.md` |
| MODIS / VIIRS / Harmonized şekil+renk ve PuOr anomali anlamı (mor=alt, turuncu=üst) | Bölüm 4, 4.1, 4.2 | [docs/TASARIM_SISTEMI.md](TASARIM_SISTEMI.md) ile uyumlu |
| Quantile vs eşit/sabit aralık kararı, aykırı değer yönetimi, `MapLegend` masaüstü + mobil wireframe (`veri yok` dahil) | Bölüm 5, 5.1, 5.2 | `1366×768` ve `375×812` taslakları eklendi |
| K4 palet seçici UX (`MethodsDrawer`, URL/`localStorage`, tüm grafiklerde senkron, baskı) | Bölüm 6, 6.1, 6.2 | K4'ün `useMapModel.ts` (`attributeFilter`) ve `renderer.ts` (`buildPaletteSignature`) içine `data-palette` eklemesi gerektiği doğrulandı |
| Light/Dark kontrast ve akromatopsi / `@media print` incelemesi | Bölüm 7 | [docs/RENK_DENETIMI.md](RENK_DENETIMI.md) kontrast/akromatopsi değerleri alıntılandı |
| K3 ve K4 için net devir listesi | Bölüm 8, 8.1, 8.2 | Tüm hedef dosya ve sorumluluklar ayrıldı |
