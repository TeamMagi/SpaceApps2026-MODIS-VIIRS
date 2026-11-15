# PyroSync — Dışa Aktarma (Export) ve Acil Müdahale Brifingi Tasarım Belirtimi

Kod ile son eşitleme: 2026-11-15 (G8/G9 yeniden tanımından sonra).

> **Kapsam:** Görev C6 çıktısı — Export akışı, durum yönetimi, `BriefingDrawer` metrik mimarisi, A4 tek sayfa yazdırma düzeni, Türkiye/Akdeniz (`S7`) yer tutucu senaryosu ve `R7` uygulama devir listesi.
> **Hedef Ekranlar:** Masaüstü `1366×768` (tam görünüm, dikey kaydırmasız) · Mobil `375×812` · A4 Dikey Yazdırma.
> **Temalar:** Koyu varsayılan + `[data-theme="light"]` açık tema + baskıda zorunlu açık palet (`@media print`).
> **Referanslar:** [TASARIM_KONSEPT.md](./TASARIM_KONSEPT.md), [TASARIM_SISTEMI.md](./TASARIM_SISTEMI.md), [UX_YAZIM.md](./UX_YAZIM.md), `web/src/styles/tokens.css`.

---

## 1. Dışa Aktarma (Export) Tasarımı

### 1.1 Mevcut Kod Gerçeği ve Tasarım Kararları

Kod tabanında bugün iki ayrı dışa aktarma yolu bulunur:

1. **İstemci tarafı arşiv dışa aktarımı (`web/src/hooks/useExportView.ts`, `web/src/lib/csv.ts`):** `useMapPoints()` üzerinden `/api/points` yanıtını alır, seçili mod (`raw` veya `harmonized`) için tarayıcıda UTF-8 BOM (`\uFEFF`) eklenmiş CSV üretir (`mapPointsToCsv` + `downloadTextFile`). Dosya adını `csvFileName(region.slug, points.period)` ile `pyrosync_{regionSlug}_{YYYY-MM|YYYY|archive}.csv` olarak oluşturur. **Eksikler:** GeoJSON yok; `harmonized` CSV içinde `vza_factor`, `frp_lower_mw`, `frp_upper_mw` yok; büyük aylarda `/api/points` `truncated: true` olabilir.
2. **Sunucu tarafı NRT/aralık uç noktası (`GET /api/export` — `crates/server/src/main.rs`, `crates/core/src/export.rs`):** `west, south, east, north`, `days` (`1..=5`) veya `start/end` (`YYYY-MM-DD`), `format=csv|geojson` parametrelerini alır. Yalnızca **harmonize** hotspotları dışa aktarır (13 sütunluk CRLF CSV veya WGS84 GeoJSON `FeatureCollection`; `frp_lower_mw` ve `frp_upper_mw` dahildir). **Eksikler:** `FIRMS_API_KEY` gerektirir (eksikse `503`); offline arşiv verisini kapsamaz; `Body` bellekte tamponlanır (`Vec<u8>`, streaming yoktur); UTF-8 BOM eklemez; dosya adını `pyrosync_{west}_{south}_{east}_{north}_{YYYYMMDD}.{ext}` gönderir.

Tasarım, kullanıcının o an haritada gördüğü kaynağa göre tek bir tutarlı arayüz sunar ve perde arkasında doğru yolu seçer (Arşiv modu → `/api/points` tabanlı istemci üretimi veya yeni arşiv uç noktası; NRT penceresi → `GET /api/export`).

### 1.2 Buton Yerleşimi Karşılaştırması ve Karar

| Aday Konum | Fitts Yasası ve Keşfedilebilirlik Değerlendirmesi | Karar |
|---|---|---|
| **Header** (`48px` üst çubuk) | Bölge seçici, canlı/demo rozeti, Yöntem çekmecesi ve dil anahtarı ile dolu. İndirme eylemini buraya koymak Z-tarama başlangıcını kalabalıklaştırır ve analiz tamamlanmadan çıktı istemeye zorlar. | **Reddedildi** |
| **MapStatusBar** (harita üstü yüzer şerit) | Halihazırda ay adımlayıcı (`Prev/Next month`) ve örneklem sayı rozetini taşır. Harita etkileşim alanını daraltır; mobilde `375px` genişliğe sığmaz. | **Reddedildi** |
| **BottomBar** (48px alt çubuk — sağ küme) | Z-tarama akışının son durak noktasıdır. Kullanıcı harita → grafik → zaman adımlamasını tamamladıktan sonra gözün doğal olarak vardığı yerde durur. Fitts Yasası açısından ekranın alt kenarına sabitlenmiş olması hedef boyutunu sonsuz dikey yükseklikle büyütür. Mevcut `BottomBar.tsx` sağ kümesinde `exportCsvShort` zaten buradadır. | **Birincil Tetikleyici** |
| **BriefingDrawer** (çekmece alt eylem çubuğu) | Acil müdahale brifingini okuyan kullanıcı, raporu inceledikten hemen sonra ham veriyi indirmek ister. Çekmece açıkken `BottomBar` arka planda veya mobilde kapalı kalır. | **İkincil Ayna Tetikleyici** |

**Karar:**
- **Birincil konum:** `BottomBar` sağ eylem kümesi (soldan sağa sırasıyla: **Bu görünümü indir** açılır seçicisi → **Bağlantıyı kopyala** → **Bu bölgeyi özetle** birincil düğmesi).
- **İkincil konum:** `BriefingDrawer` altındaki sabit eylem çubuğunda aynı indirme bileşeninin kompakt sürümü (`CSV` / `GeoJSON` indir + `Yazdır`).
- **Doygunluk bütçesi kuralı (`TASARIM_SISTEMI.md`):** `--color-accent` (`#3cc8f0`) yalnızca Ham/Harmonize anahtarı ve 2012 işaretçisi içindir. İndirme ve brifing düğmeleri nötr/yüzey yükseltilmiş stil (`--color-surface-raised`, `--color-border`, `--color-text`) kullanır.

### 1.3 Biçim Seçim Kontrolü: Segmented Control vs Açılır Menü

İki karşılıklı dışlayıcı biçim (`CSV` ve `GeoJSON`) bulunduğundan Hick Yasası maliyeti düşüktür. Ancak `BottomBar` (`48px` yükseklik, dar yatay bütçe) ile `BriefingDrawer` (`420px` genişlik) farklı alan kısıtlarına sahiptir:

- **Karar (Hibrit Desen):**
  - `BottomBar` içinde tek tıklamalı **"Bu görünümü indir ▾"** düğmesi bir **Popover Panel** (`--z-popover: 40`) açar. Popover içinde **2 seçenekli Segmented Control (`CSV` | `GeoJSON`)** yer alır; altında seçili biçimin tek satırlık açıklaması, dışa aktarılacak kapsam/satır önizlemesi ve tek **"İndir"** onay düğmesi bulunur. Böylece `BottomBar` tek düğme genişliğinde kalır, yanlışlıkla ağır indirme başlatılmaz ve kullanıcı satır sayısını indirime basmadan görür.
  - `BriefingDrawer` içinde yer daha geniş olduğu için **Segmented Control (`CSV` | `GeoJSON`)** doğrudan satır içi (inline) gösterilir ve yanındaki **"Veriyi indir"** düğmesi tek tıkla başlatır.

#### Biçim Seçenekleri ve Tek Satırlık Açıklamalar (EN / TR)

| Biçim | Kısa Etiket | Tek Satırlık Açıklama (TR) | Single-Line Explanation (EN) | Teknik Karşılık |
|---|---|---|---|---|
| **CSV** | `CSV` | `Excel, LibreOffice ve tablo araçları için nokta listesi (UTF-8).` | `Point table for Excel, LibreOffice, and spreadsheets (UTF-8).` | `text/csv; charset=utf-8` (13 sütun `CSV_HEADER` + istemci/sunucu BOM önerisi) |
| **GeoJSON** | `GeoJSON` | `QGIS, ArcGIS ve CBS araçları için WGS84 konum katmanı.` | `WGS84 spatial layer for QGIS, ArcGIS, and GIS tools.` | `application/geo+json` (`FeatureCollection`, `[lon, lat]` noktalar) |

### 1.4 Dosya Adı Uzlaştırması ve Görkem / R7 Sunucu Notu

Görev metni ve mevcut kod arasında belirgin bir dosya adı uyuşmazlığı vardır:

| Kaynak | Mevcut Dosya Adı Kalıbı | Örnek Çıktı | Fark / Sorun |
|---|---|---|---|
| **Görev C6 & `UX_YAZIM.md`** | `pyrosync_{region}_{date}.csv` | `pyrosync_greece_2023-08.csv` | Yalnızca CSV örneği var; GeoJSON uzantısı ve NRT gün penceresi netleştirilmemiş. |
| **Mevcut Frontend (`csvFileName` — `web/src/lib/format.ts`)** | `pyrosync_{regionSlug}_{YYYY-MM\|YYYY\|archive}.csv` | `pyrosync_greece_2023-08.csv` | Sunucu NRT yanıtıyla uyuşmuyor; GeoJSON üretmiyor. |
| **Mevcut Backend (`export_response` — `crates/server/src/main.rs:1351-1367`)** | `pyrosync_{west}_{south}_{east}_{north}_{YYYYMMDD}.{ext}` | `pyrosync_19.5_34.8_29.6_41.7_20261115.geojson` | Ham koordinatlar dosya adını okunaksızlaştırıyor (`19.5_34.8_...`); bölge adı (`region` slug) yok; cache `HIT` olsa bile dosya adındaki tarih `Utc::now()` ile bugüne basılıyor. |

**Önerilen Birleşik Dosya Adı Standardı:**
- **Bölge tanımlıysa:** `pyrosync_{region-slug}_{kapsam}_{YYYYMMDD}.{csv|geojson}`
  - Arşiv ayı örneği: `pyrosync_turkey_2021-08_20261115.csv` (veya arşiv dönemi zaten tarih içerdiğinden kısa biçim: `pyrosync_turkey_2021-08.csv`).
  - NRT penceresi örneği: `pyrosync_turkey_son-7gun_20261115.geojson` (veya ISO tarih aralığı: `pyrosync_turkey_2026-11-08_2026-11-15.geojson`).
- **Özel / çizilmiş bbox (region slug yoksa):** `pyrosync_bbox_{west}_{south}_{east}_{north}_{kapsam}.{csv|geojson}` (koordinatlar 2 ondalık basamağa yuvarlanmış, örn. `27.30_36.50_32.60_37.30`).

> [!IMPORTANT]
> **Görkem (G10/Backend) ve R7 (Frontend) için Sunucu Değişiklik Notu:**
> 1. `GET /api/export` sorgusuna isteğe bağlı `region: Option<String>` parametresi eklenmeli; yalnızca güvenli slug karakterlerine (`[a-z0-9-]`) izin verilerek `Content-Disposition` başlığında `pyrosync_{region}_{window}_{YYYYMMDD}.{ext}` üretilmeli (`region` yoksa yuvarlatılmış bbox fallback).
> 2. `crates/core/src/export.rs` içindeki `to_csv` çıktısının başına Windows Excel Türkçe yerel ayarlarında (`;` vs `,` ve Türkçe karakterler) sorun çıkmaması için UTF-8 BOM (`\u{FEFF}`) eklenmesi önerilir; tarayıcı/QGIS uyumludur.
> 3. Önbellek `HIT` durumunda da deterministik dosya adı için `Utc::now()` yerine sorgu penceresinin bitiş tarihi (`end` veya istek zamanı) kullanılmalıdır.
> 4. `GOREVLER2.md` G10'da akışlı yanıt (`streaming`) hedeflenmiş olsa da bugün `main.rs` tam `Vec<u8>` tamponlaması yapar; bu nedenle arayüz yüzde bazlı ilerleme çubuğu yerine belirsiz (`indeterminate`) yükleme veya satır önizlemesi göstermelidir.

### 1.5 Dışa Aktarılan Kapsam (Bbox / Tarih Aralığı) ve Satır Sayısı Önizlemesi

Popover (ve `BriefingDrawer` export alanı) indirme öncesi tam şeffaflık sağlar:

```
┌──────────────────────────────────────────────────────────┐
│ Bu görünümü indir                                    [×] │
├──────────────────────────────────────────────────────────┤
│ Biçim:   (● CSV )  ( GeoJSON )                           │
│          Excel, LibreOffice ve tablo araçları için       │
│          nokta listesi (UTF-8).                          │
├──────────────────────────────────────────────────────────┤
│ Kapsam:  Türkiye · Ağustos 2021 (UTC)                    │
│ Alan:    26,0°D – 45,0°D, 36,0°K – 42,0°K                │
│ Katman:  Harmonize (çift sayım ayıklanmış, VZA düzeltmeli)│
│ Kayıt:   6.520 nokta · ~1,1 MB tahmin                    │
│ Dosya:   pyrosync_turkey_2021-08.csv                     │
├──────────────────────────────────────────────────────────┤
│ [!] Not: Harita görünümü 4.000 nokta sınırıyla kesildi   │
│     (sunucu sınırı). Tam veri için daraltılmış alan seçin.│  <- yalnız truncated === true ise
├──────────────────────────────────────────────────────────┤
│                                       [ İptal ] [ İndir ]│
└──────────────────────────────────────────────────────────┘
```

- **Bbox ve tarih aralığı kuralı:** Her zaman kullanıcının o an aktif gördüğü kapsam dışa aktarılır:
  - Arşiv incelemesinde (`selectedMonth != null`): aktif bölgenin `bbox` alanı + `selectedMonth` ayı (ör. `2021-08 UTC`) + aktif mod (`raw` vs `harmonized`, ancak sunucu `/api/export` şeması seçilirse yalnızca harmonize döner — bu durum satırda açıkça yazılır).
  - NRT modunda: haritanın güncel viewport veya bölge `bbox` değeri + `nrtDays` / 7 günlük UTC aralığı (`start..end`).
- **Satır sayısı önizlemesi:**
  - Arşiv görünümünde `/api/points` yanıtı zaten bellekte olduğundan satır sayısı kesin hesaplanır (`raw.length` veya `harmonized.length`) ve bayt tahmini gösterilir (`yaklaşık {n} KB / MB`).
  - `/api/points` `truncated: true` döndüyse (`DEFAULT_POINTS_LIMIT = 4000` sunucu sınırı aşıldığında), indirmeden önce uyarılır (`UX_YAZIM.md` sakin tonuyla: *"Bu görünüm {limit} nokta sınırıyla kısaltıldı; indirilen dosya alt kümeyi içerir."*).
  - NRT sorgusunda `/api/stats` verisi yüklüyse `stats.totals.harmonizedRecords` önizleme olarak gösterilir; henüz çekilmediyse `"Satır sayısı indirme sırasında hesaplanır."` yazılır.

---

## 2. Export Durumları (`idle`, `preparing`, `success`, `error`, `empty`, `demo`, `offline`)

Tüm bildirim metinleri `docs/UX_YAZIM.md` kurallarına uygundur: ünlem yok, emoji yok, `"Hata oluştu"` / `"Lütfen bekleyin"` / `"Yükleniyor..."` yasak; sayı biçimleri TR virgül/nokta (`6.520`), EN nokta/virgül (`6,520`) ve UTC zaman damgalıdır.

| Durum | Tetikleyici / Koşul | Masaüstü (`1366×768`) Davranışı | Mobil (`375×812`) Davranışı | Kullanıcı Metni (TR / EN) | Erişilebilirlik (`aria`) |
|---|---|---|---|---|---|
| **`idle`** | Varsayılan durum veya seçim yapılıyor | Popover/çekmece içinde kapsam + satır önizlemesi ve etkin `İndir` düğmesi görünür. | Tam genişlikli alt sayfa (`bottom sheet`) içinde aynı kontroller, `44px` dokunma hedefleri. | TR: `Bu görünümü indir` · `Seçili kapsam indirilecek.`<br>EN: `Download this view` · `Downloads the active scope.` | `button` (`aria-expanded`, `aria-haspopup="dialog"`) |
| **`preparing`** | İndir tıklandı (`useExportView` veya `fetch('/api/export')` sürüyor) | İndir düğmesi kilitlenir; içinde `14px` dönen belirteç + belirsiz (`indeterminate`) ince ilerleme çizgisi görünür (`animate` yalnız `prefers-reduced-motion: no-preference` iken). Sunucu streaming yapmadığı için `%` gösterilmez. | Alt sayfa kapanmaz; düğme tam genişlikte `Hazırlanıyor` durumuna geçer, kapatma dışı eylemler kilitlenir. | TR: `Dosya hazırlanıyor — {n} kayıt {format} biçimine dönüştürülüyor.`<br>EN: `Preparing file — formatting {n} records as {format}.` | `aria-busy="true"`, `role="status"`, `aria-live="polite"` |
| **`success`** | Blob oluştu ve tarayıcı indirmeyi tetikledi | Popover kapanır (veya çekmecede kalır); `BottomBar` üzerinde 6 sn süreli ince durum şeridi dosya adını gösterir (`--color-status-live` değil nötr yüzey + onay ikonu). | Ekran altından `48px` banner görünür; kaydırma ile kapatılabilir. | TR: `İndirildi: {filename} ({rows} satır).`<br>EN: `Downloaded: {filename} ({rows} rows).` | `role="status"`, `aria-live="polite"` |
| **`error`** | Ağ kesildi, `HTTP 4xx/5xx` (örn. `FIRMS_API_KEY` veya arşiv dizini eksikliği `503`, istek zaman aşımı `504`, büyük bölge/arşiv sınırı aşımı `413`) | Açıklayıcı durum mesajı (`--color-status-error` sol kenar çizgisi + metin) ve doğrudan **`Yeniden dene`** düğmesi popover içinde kalır; kullanıcı seçimi sıfırlanmaz. | Alt sayfa açık kalır; hata metni + tam genişlik `44px` `Yeniden dene` düğmesi. | TR: `Dışa aktarma tamamlanamadı ({reason}). Bağlantıyı kontrol edip yeniden deneyin.`<br>EN: `Export did not complete ({reason}). Check the connection and retry.` | `role="alert"`, odak hata özetine taşınır |
| **`empty`** | Seçili ay/bbox için 0 tespit (sunucu 1 satır başlık CSV veya `features: []` GeoJSON döndürürse veya `points.harmonized.length === 0`) | İndirmeden önce `rows === 0` ise `İndir` düğmesi yerine bilgi kutusu gösterilir; boş dosya gereksiz yere indirilmez. Sunucu yanıtı 0 satır gelirse dosya kaydedilmez, boş kapsam bildirimi verilir. | Alt sayfada aynı bilgi bloğu + `"Başka ay seç"` yönlendirmesi. | TR: `Bu bölge ve tarih aralığında dışa aktarılacak tespit bulunmuyor.`<br>EN: `No detections exist in this region and date range to export.` | `role="status"`, `aria-live="polite"` |
| **`demo`** | Uygulama `demo` kipinde (`mode === 'demo'`; Not: `sample-data.ts` henüz repoda yoktur, `K1` sonrası bağlanır) | Popover başında `--color-status-demo` noktalı şerit görünür. İndirilen dosya adının sonuna `_demo` eklenir (`pyrosync_turkey_2021-08_demo.csv`) ve CSV ilk yorum/sütun veya GeoJSON üst nesne alanı (`"demo": true`) ile etiketlenir. | Alt sayfa üstünde tam genişlik `Örnek veri` etiketi; aynı `_demo` dosya adı kuralı. | TR: `Örnek veri kipi etkin — indirilen dosya örnek kayıtlar içerir.`<br>EN: `Sample data mode active — downloaded file contains sample records.` | `role="note"` |
| **`offline`** | Sunucuya ulaşılamıyor ve istenen kapsam önbellekte yok (NRT veya önbelleksiz arşiv) | `İndir` düğmesi pasif (`disabled`), yalnızca yerel tarayıcı önbelleğindeki `/api/points` görünümü varsa onu dışa aktarma seçeneği açık bırakılır. | Alt sayfada pasif düğme + çevrimdışı durum açıklaması. | TR: `Sunucu bağlantısı yok — çevrimdışı durumda yalnız önbellekteki aktif görünüm indirilebilir.`<br>EN: `Server offline — only the cached active view can be exported offline.` | `aria-disabled="true"` + açıklama bağlantısı (`aria-describedby`) |

---

## 3. Acil Müdahale Brifingi (Briefing) Tasarımı

### 3.1 Konteyner Kararı: Modal vs Drawer vs Kalıcı Kenar Çubuğu

| Seçenek | Değerlendirme | Karar |
|---|---|---|
| **Modal** | Haritayı ve 2012 geçiş grafiğini tamamen karartır; kullanıcı brifingdeki sayıyı okurken arkasındaki bölge/harita bağlamını kıyaslayamaz. | **Reddedildi** |
| **Kalıcı Kenar Çubuğu (Sidebar)** | Masaüstü `1366×768` düzeninde harita (`--w-map: 58%`) ve sağ analiz sütunu zaten iki bölmelidir; üçüncü kalıcı sütun haritayı ~`420px` daraltır. | **Reddedildi** |
| **`BriefingDrawer` (`<Drawer id="briefing">`)** | Kod tabanında `Drawer.tsx` (`--w-drawer: 420px` sağdan kayan çekmece, `>= 640px`) ve `BriefingDrawer.tsx` halihazırda mevcuttur. Haritanın sol/orta kısmını görünür bırakır, odak tuzağı (`focus trap`), `Escape` ile kapanma ve yazdırma seçicisi (`[data-drawer="briefing"]`) altyapısına sahiptir. | **Kabul Edildi (Korunup Genişletilecek)** |

- **Mobil (`375×812`) uyarlaması:** Mevcut `Drawer.module.css` mobilde `max-height: 90dvh` alt sayfa (bottom sheet) kullanır. Acil müdahale brifingi 8+ metrik, mini grafik ve yazdırma/indirme aksiyonları içerdiği için mobilde **tam ekran (`height: 100dvh; max-height: 100dvh; border-radius: 0`)** veya kolay kaydırılabilir tam yükseklik alt sayfa (`BriefingDrawer` için özel `.sheetFull` sınıfı) kullanılmalı; böylece tutamaç (`48px`) alanı boşa harcanmaz ve tek sütunlu metrik okuma alanı maksimize edilir.
- **Tetikleyici:** `BottomBar.tsx` içindeki birincil eylem düğmesi `briefMe` (`"Bu bölgeyi özetle"` / `"Brief me on this region"`). Klavye kısayolu olarak `B` önerilir (giriş alanları dışındayken).

### 3.2 Brifing Metrik Sırası ve Veri Kaynağı Eşlemesi

Brifing içeriği şu hiyerarşiyi izler:
1. **Kimlik ve Kapsam Başlığı** (Bölge, seçili ay veya son 7 gün penceresi, veri kip rozeti, UTC oluşturma zamanı)
2. **Temel Bulgu Özeti** (Ham → Harmonize farkı + anomali tek cümlesi)
3. **Çekirdek Acil Müdahale Metrikleri (C6 + R7 tam listesi, sırasıyla 1–9)**
4. **Statik Zaman Serisi Mini Grafiği** (Son 12 ay veya yıllık Ham vs Harmonize mini sparkline)
5. **Sabit Alt Eylem Çubuğu** (Format seçici `CSV | GeoJSON` + İndir + `Yazdır (A4)` + `Bağlantıyı kopyala`)

#### Tam Metrik Listesi (Sıralı)

> **Doğruluk Notu:** Aşağıda `[Bugün Hesaplanabilir]` olarak işaretlenen **7 metrik**, uygulamanın halihazırda yüklediği `useRegionData()`, aynı dosyada tanımlı `useMapPoints()` (`web/src/hooks/useRegionData.ts`), `useNrtData()` ve `web/src/lib/radialCalendar.ts` verilerinden yeni backend endpointi olmadan hesaplanabilir. Kalan alanlardaki eksik parçalar (`S7`, `G9` TypeScript eşlemesi, `7 gün` NRT pencere limiti) açıkça belirtilmiştir.

| Sıra | Metrik (TR / EN) | Formül / Tanım | Veri Kaynağı ve Durumu | Gösterim Biçimi (`UX_YAZIM.md`) | G9 Belirsizlik Gösterimi |
|---|---|---|---|---|---|
| **1** | **Tarihsel ortalama vs şu an**<br>Historical average vs current | Seçili aydaki (`M`) mevcut `harmonizedCount` ile aynı takvim ayının 2003–2024 tarihsel ortalama değeri (`baselineMean(M)`) yan yana karşılaştırılır. Yıllık görünümde `briefing.meanAnnualHarmonized` vs seçili yıl `harmonized`. | `[Bugün Hesaplanabilir]`<br>`useRegionData().months` + `computeMonthlyBaselines(months)` (`web/src/lib/radialCalendar.ts`) ve seçili hücre `useSelectedMonthCell()`. | `6.520 vs 2.410 ort.`<br>(`6,520 vs 2,410 avg.`) | Aylık FRP ortalaması gösterildiğinde (`CalendarCell` NRT veya G9 sonrası arşiv): `45,2 MW (olası aralık 38,7–51,8)` / `45.2 MW (likely 38.7–51.8)`. *Not: `archive_api.rs` `MonthEntry` şu an `mean_frp` Bounds üretmez.* |
| **2** | **Tarihsel ortalamadan sapma**<br>Deviation from historical average | `deltaPct = ((current - baselineMean) / baselineMean) * 100` ve z-skoru `z = (current - baselineMean) / baselineStd`. | `[Bugün Hesaplanabilir]`<br>`computeMonthNormalDeltaPct()` (`web/src/lib/radialCalendar.ts`) + `MonthCell.anomalyHarmonized` (`useRegionData().months`). | `+%170,5 (+2,3σ)`<br>(`+170.5% (+2.3σ)`) | Sapma % ve σ belirgin sayılar olarak gösterilir; baz yıllar 2003–2024 tam yıllardan hesaplandığı alt notla belirtilir. |
| **3** | **Anomali durumu**<br>Anomaly status | Seçili ay veya güncel pencerenin `anomalyHarmonized` z-skoruna göre 7 kademeli anomali eşiği (`--anom-neg-3` .. `--anom-pos-3`): `≥ +2.0σ` Belirgin artış, `+1.0..+2.0σ` Ortalamanın üstü, `-1.0..+1.0σ` Normal aralık, `≤ -1.0σ` Ortalamanın altı. | `[Bugün Hesaplanabilir]`<br>`MonthCell.anomalyHarmonized` (`useRegionData`) veya NRT `/api/harmonize` `calendar.anomaly`. | Renkli kare rozet (`--anom-*`) + sakin etiket:<br>TR: `Ortalamanın +2,3σ üstü`<br>EN: `+2.3σ above baseline` | Güven dar/düşük olduğunda (ör. tek uydulu dönem veya düşük örneklem `effort < eşik`): *"Düşük örneklem — belirsizlik geniş"* etiketi eklenir (`NrtCalendar.uncertainty` üzerinden — `api.ts` eşlemesi gerekir). |
| **4** | **En yoğun aylar**<br>Peak months | Aktif seri içindeki ayları `harmonizedCount` değerine göre azalan sırala: en yüksek 1 pik ay + ilk 3 yoğun ay (`YYYY-MM`: `count`). | `[Bugün Hesaplanabilir]`<br>`useRegionData().months` (ayrıca `briefing.peakMonth` bugün en tepe ayı zaten döner). | `1. Ağu 2021 (6.520)`<br>`2. Tem 2021 (3.890)`<br>`3. Eyl 2020 (2.140)` | Her ayın yanında ham sayı parantez içi değil, ayrı küçük etiketle isteğe bağlı (`Ham: 8.110 → Harm: 6.520`). |
| **5** | **Aktif sensörler**<br>Active sensors | Kapsam içinde tespit üreten platform/sensör listesi ve her birinin katkı payı. Arşivde `rawModis` / `rawViirs` (`MODIS`, `VIIRS`); NRT'de uydu dökümü (`MODIS_NRT`, `VIIRS_SNPP_NRT`, `VIIRS_NOAA20_NRT`, `VIIRS_NOAA21_NRT`). | `[Bugün Hesaplanabilir]` (Arşiv MODIS/VIIRS & NRT 1–5 gün)<br>Arşiv: `useRegionData().totals` (`modis`, `viirs`) ve `yearly[y]`.<br>NRT: `useNrtData().stats.sensorBreakdown`. | Renk noktalı chip'ler (`--color-sensor-modis`, `--color-sensor-viirs`):<br>`MODIS (%28,4)` · `VIIRS (%71,6)` | Çift sayım paylaşımı varsa: `Çakışan (%14,2)` halkası (`--color-duplicate-ring`) ayrı rozette gösterilir. |
| **6** | **En yoğun sensör**<br>Dominant sensor | Aktif penceredeki tespit sayısı (veya FRP payı) en yüksek sensör/platform: `argmax_s(count_s)` ve toplam ham/eşleşmiş içindeki oranı. | `[Bugün Hesaplanabilir]`<br>Arşiv: `rawModis` vs `rawViirs` (`useRegionData`); NRT: `argmax(stats.sensorBreakdown)` (`useNrtData`). | `VIIRS — 20.140 tespit (%71,6)`<br>(`VIIRS — 20,140 detections (71.6%)`) | Ham tespit payı ile VZA açısı düzeltmeli FRP payı farklı sensörü işaret ederse iki satırlı not gösterilir. |
| **7** | **Son 7 gün tespit**<br>Last 7 days detections | `today - 7 gün .. today` (UTC) penceresindeki harmonize toplam tespit sayısı + toplam ve ortalama FRP. | `Yeni hesap / backend pencere uyumu gerekir`<br>**Gerçek kod gap'i:** `parse_time_window` (`crates/server/src/main.rs:2081-2120`) `days` parametresini **`1..=5`** arasıyla sınırlar; `useAppStore` varsayılanı `nrtDays: 2`'dir. Çare: NRT çağrısını `start` & `end` (7 günlük UTC aralık) ile yapmak ya da `DayRange::new` üst sınırını `7`'ye çıkarmak. | `412 harmonize tespit (son 7 gün UTC)`<br>(`412 harmonized detections (last 7d UTC)`) | `frp.harmonizedSum` veya `frp.mean` için G9 alt/üst sınırı: `45,2 MW (olası aralık 38,7–51,8)` (`WireHarmonizedPoint` üzerine `frp_lower`/`frp_upper` TS eşlemesi gerekir). |
| **8** | **Trend (artış/azalış)**<br>Trend (increase / decrease) | NRT: son 7 günün ilk yarısı vs ikinci yarısı (veya önceki 7 günle kıyas) `%` değişim ve yön oku (`Artış +X%` / `Azalış -X%` / `Yatay ±%5`).<br>Arşiv: son 3 tam yıl veya seçili ayın bir önceki yıla göre değişimi (`(Y - Y_{-1}) / Y_{-1}`). | `Yeni hesap gerekir`<br>Verisi mevcut (`yearly` ve NRT `temporal` / günlük gruplama), ancak trend eğimi hesaplayan saf yardımcı fonksiyon (`computeBriefingTrend`) henüz yazılmamış. | `Artış: +%18,4 (önceki döneme göre)`<br>(`Increase: +18.4% vs prior period`) | Küçük örneklemlerde (`n < 20`) oran yanıltıcı olabileceğinden `"Düşük örneklem — trend yorumlanamaz"` korunması eklenir. |
| **9** | **Harmonizasyon ve VZA belirsizliği**<br>Harmonization & VZA uncertainty | Ham → Harmonize indirgeme (`doubleCountShare`), bakış açısı FRP düzeltmesi (`frpAngleCorrectionPct`), belirsizlik bandı (`frp_lower_mw`..`frp_upper_mw`, G9 varsayılan `±1σ` `~%68` güven aralığı; `uncertainty.z` ile yapılandırılabilir). | `[Kısmen Mevcut — TS eşlemesi gerekir]`<br>`doubleCountShare` ve `frpAngleCorrectionPct` bugün `useRegionData().briefing` içinde var. Backend `HarmonizedHotspot` (`models.rs`) `frp_lower` ve `frp_upper` alanlarını (`ModisOnly` için `modis_scale_sigma = 0.10` dahil; tespit bazında `calibration_ratio` yok), `CalendarCell` ise `total_frp`, `total_frp_lower`/`upper`, `frp_per_detection_eq` alanlarını zaten gönderiyor; ancak `web/src/services/api.ts` (`WireHarmonizedPoint`) bunları henüz çözmüyor. | `Çift sayım: -%17,7 · VZA: +%14,2`<br>`Ort. FRP: 45,2 MW (olası aralık 38,7–51,8)` | Doğrudan G9 `±1σ` (`~%68`, `DEFAULT_Z = 1.0`, `uncertainty.z` ile yapılandırılabilir) aralığını, `modis_scale_sigma` + VIIRS öncesi `total_frp` bütünlük faktörünü (`k_c`, `calibration.rs`) ve `UNCERTAINTY_NOTE` özetini gösterir. |

*(Türkiye özel senaryosu için ek satır: S7 veri seti eklendiğinde, 2021 Akdeniz yangın haftası karşılaştırması **"S7 verisi gerekir"** etiketiyle ayrı bir örnek durum kartında sunulur — bkz. Bölüm 6.)*

---

## 4. Brifing Wireframe'leri (1366×768 Masaüstü, 375×812 Mobil ve Durumlar)

### 4.1 Masaüstü `1366×768` — `hazır` Durumu (`BriefingDrawer` sağda `420px`)

```
+----------------------------------------------------------------------+
| PyroSync  [ Türkiye (Antalya/Muğla) v ]  • Örnek/Canlı   Yöntem  TR  | 48px
+------------------------------------+---------------------------------+
|                                    |  Bölge Brifingi             [×] |
|                                    |---------------------------------|
|   [ HARİTA ARKA PLANDA GÖRÜNÜR ]   |  Türkiye · Ağu 2021 (UTC)       |
|   (Sol %69 alan açık kalır;        |  Kaynak: Arşiv · Harmonize      |
|    kullanıcı noktaları ve          |  Oluşturma: 2026-11-15 08:45 UTC|
|    2012 geçişini görmeye           |---------------------------------|
|    devam eder)                     |  BULGU                          |
|                                    |  Ham kayıt 8.110; çift sayım    |
|                                    |  ve uydu çakışması ayıklandıktan|
|                                    |  sonra 6.520 tekil ateş kaldı.  |
|                                    |---------------------------------|
|                                    |  ACİL MÜDAHALE ÖZETİ            |
|                                    |  1. Tarihsel ort. vs şu an      |
|                                    |     6.520 vs 2.410 ort.         |
|                                    |  2. Ortalamadan sapma           |
|                                    |     +%170,5 (+2,3σ)             |
|                                    |  3. Anomali durumu              |
|                                    |     [■] Ortalamanın +2,3σ üstü  |
|                                    |  4. En yoğun aylar              |
|                                    |     Ağu 2021 · Tem 2021 · Eyl 20|
|                                    |  5. Aktif sensörler             |
|                                    |     (● MODIS %28) (● VIIRS %72) |
|                                    |  6. En yoğun sensör             |
|                                    |     VIIRS — 20.140 (%71,6)      |
|                                    |  7. Son 7 gün tespit (NRT)      |
|                                    |     412 tespit (2026-11-08..15) |
|                                    |  8. Trend                       |
|                                    |     Artış: +%18,4               |
|                                    |  9. VZA / G9 belirsizlik bandı  |
|                                    |     45,2 MW (olası 38,7–51,8)   |
|                                    |---------------------------------|
|                                    |  [ Mini Sparkline SVG (12 ay) ] |
|                                    |---------------------------------|
|                                    |  (● CSV | GeoJSON)   [ İndir ]  |
|                                    |  [ Bağlantıyı kopyala ] [Yazdır]|
+------------------------------------+---------------------------------+
```

### 4.2 Mobil `375×812` — Tam Ekran Çekmece (`hazır` Durumu)

```
+-----------------------------------+
| Bölge Brifingi                [×] | 48px sabit başlık
| Türkiye · Ağu 2021 · Harmonize    |
+-----------------------------------+
| BULGU                             |
| Ham 8.110 → 6.520 tekil ateş.     |
|                                   |
| +-------------------------------+ |
| | Tarihsel ort. vs şu an        | |
| | 6.520  vs  2.410 ort.         | |
| +-------------------------------+ |
| | Sapma & Anomali               | |
| | +%170,5 (+2,3σ) [■]           | |
| +-------------------------------+ |
| | En yoğun aylar                | |
| | 1. Ağu 2021 (6.520)           | |
| | 2. Tem 2021 (3.890)           | |
| | 3. Eyl 2020 (2.140)           | |
| +-------------------------------+ |
| | Aktif sensörler · En yoğun    | |
| | MODIS %28 · VIIRS %72 (baskın)| |
| +-------------------------------+ |
| | Son 7 gün · Trend (UTC)       | |
| | 412 tespit · Artış +%18,4     | |
| +-------------------------------+ |
| | VZA / Belirsizlik (G9)        | |
| | 45,2 MW (olası 38,7–51,8)     | |
| +-------------------------------+ |
| | [   Mini Sparkline (343px)  ] | |
| +-------------------------------+ |
|                                   |
+-----------------------------------+
| (● CSV | GeoJSON)  [ Veriyi indir]| 96px sabit alt çubuk
| [ Bağlantıyı kopyala ]  [ Yazdır ]| (dokunma >= 44px)
+-----------------------------------+
```

### 4.3 Brifing Durumları (`boş`, `üretiliyor`, `hazır`, `hata`)

| Durum | Görünüm ve Wireframe Özeti | Kopya (TR / EN) |
|---|---|---|
| **`boş`** | Bölge veya dönem seçilmediğinde ya da seçili dönemde 0 tespit varsa metrik grid yerine merkezlenmiş sakin kart ve birincil eylem (`Bölge seç` / `Tüm yılları göster`). | TR: `Özet üretmek için bir bölge veya aktif dönem seçin.` / `Bu dönemde kayıtlı tespit yok.`<br>EN: `Select a region or active period to build a briefing.` / `No detections recorded in this period.` |
| **`üretiliyor`** | Çekmece başlığı görünür; altında sabit yükseklikte (`72px` × 4) iskelet kartlar (`prefers-reduced-motion: reduce` iken parıltı animasyonu kapalı, sabit `--color-surface-sunken`). Alt eylem düğmeleri `disabled`. | TR: `Bölge özeti ve metrikler hesaplanıyor.`<br>EN: `Calculating region summary and metrics.` |
| **`hazır`** | Yukarıdaki tam masaüstü/mobil wireframe düzeni; yazdırma ve indirme düğmeleri etkin. | *(Bkz. Bölüm 3.2 ve 4.1)* |
| **`hata`** | Çekmece üstünde `--color-status-error` sol şeritli uyarı kartı + sunucu/ağ nedeni + `Yeniden dene` düğmesi. Eğer arşiv verisi bellekte varsa sadece eksik NRT satırları (`Son 7 gün tespit`) `"Veri alınamadı — yeniden dene"` durumuna düşer; tüm brifing kilitlenmez. | TR: `Brifing verileri alınamadı ({reason}). Arşiv metrikleri gösteriliyorsa incelemeye devam edebilir veya yeniden deneyebilirsiniz.`<br>EN: `Could not load briefing data ({reason}). Available archive metrics remain visible, or you can retry.` |

---

## 5. Yazdırma Görünümü (`@media print` — A4 Dikey Tek Sayfa)

Mevcut kodda `Drawer.module.css` (`#root`, `[data-drawer="methods"]`, `.backdrop` ve yalnızca `.close, .footer` öğelerini gizler, drawer gölgesini kaldırır) yazdırma için iyi bir başlangıç içerir; ancak bugün `BriefingDrawer.module.css` içinde `.actions` gizlenmemektedir (`R7` tarafından `@media print` altında `.actions { display: none; }` eklenmelidir), renk değişkenleri koyu temada kalabilir, A4 sayfa marjı tanımlı değildir, mini grafik, sayfa URL'si, oluşturma zamanı ve G9 belirsizlik alt bilgisi yoktur.

### 5.1 Zorunlu `@media print` Kuralları

1. **Sayfa boyutu ve tek sayfa hedefi:**
   - `@page { size: A4 portrait; margin: 12mm 14mm; }`
   - Tüm brifing içeriği kompakt `2×4` veya `2×5` ızgara kullanarak **1 A4 dikey sayfaya** (`~273mm` kullanılabilir yükseklik) sığar.
2. **Zorunlu açık palet ve siyah-beyaz (S/B) okunabilirlik:**
   - `:root, [data-theme="dark"], [data-theme="light"] { color-scheme: light; }` ve `body`, `[data-drawer="briefing"]` içinde Bölüm 7.2'de önerilen print token'ları (`var(--color-print-bg)` arka plan, `var(--color-print-text)` metin, `var(--color-print-muted)` ikincil metin, `1pt solid var(--color-print-border)` kenarlıklar) zorlanır.
   - Sensör ve anomali göstergeleri renksiz S/B lazer yazıcıda da ayırt edilebilmesi için yalnız renge dayanmaz: MODIS için içi boş daire (`○`), VIIRS için dolu kare (`■`), anomali seviyesi için metinsel `+2,3σ` etiketi ve SVG çizgilerde düz vs kesikli çizgi (`stroke-dasharray: 4 3` Ham, düz `2px` siyah çizgi Harmonize) kullanılır.
3. **Gizlenen öğeler (`display: none !important`):**
   - `#root` (harita, zaman adımlayıcı, ham/harmonize anahtarı, alt çubuk, kenar çizelgeleri)
   - `[data-drawer="methods"]` ve `.backdrop`
   - Çekmece kapatma (`[×]`, `Drawer.module.css` `.close, .footer`), `CSV/GeoJSON` seçici, `İndir`, `Yazdır`, `Bağlantıyı kopyala` düğmeleri (`BriefingDrawer.module.css` `@media print` içine R7 tarafından `.actions { display: none; }` eklenir)
4. **Gösterilen ve baskıya özel eklenen öğeler:**
   - **Baskı Üst Başlığı:** PyroSync logosu/adı + Bölge adı & Bbox koordinatları + Kapsam (`Ağustos 2021 UTC` veya `Son 7 gün`) + Veri Kipi (`Arşiv` / `Canlı NRT` / `Örnek Veri`) + Oluşturma Zamanı (`YYYY-MM-DD HH:MM UTC`).
   - **Bulgu Kartı + 9 Metrik Tablosu** (tabular-nums ile hizalı).
   - **Statik Mini SVG Sparkline:** JavaScript/Canvas bağımlılığı olmadan doğrudan DOM'a basılan saf `<svg>` (vektör olduğu için 300+ dpi keskin); ham seri kesikli gri-koyu çizgi, harmonize seri kalın siyah çizgi, 2012 eşiği varsa dikey noktalı çizgi.
   - **Baskı Alt Bilgisi (Footer):** `disclaimer` ("Harmonize değerler model tahminidir, yer gerçeği değildir.") + G9 `±1σ` (`~%68`, `uncertainty.z` ile yapılandırılabilir) VZA, `modis_scale_sigma` ve VIIRS öncesi `total_frp` bütünlük (`k_c`) belirsizliği notu + sayfanın paylaşılabilir tam URL'si (`window.location.href`).
5. **Sayfa kırılım kuralları:**
   - Her metrik kartı, sparkline kapsayıcısı ve alt bilgi bloğu: `break-inside: avoid; page-break-inside: avoid;`.
   - Başlıklardan hemen sonra sayfa bölünmesini engellemek için: `break-after: avoid;`.

### 5.2 A4 Dikey Yazdırma ASCII Wireframe

```
+======================================================================+
| PYROSYNC · BÖLGE ACİL DURUM BRİFİNGİ                                 |
| Bölge: Türkiye (26,0°D–45,0°D, 36,0°K–42,0°K)   Kapsam: Ağu 2021 UTC |
| Kaynak: Arşiv · Harmonize (MODIS + VIIRS)    Tarih: 2026-11-15 08:45 |
+======================================================================+
| BULGU ÖZETİ                                                          |
| Ham tespit 8.110 iken çift sayım ve çoklu uydu geçişi ayıklandıktan   |
| sonra 6.520 tekil yangın tespiti hesaplandı (-%19,6).                |
+----------------------------------------------------------------------+
| TEMEL METRİKLER                                                      |
| +----------------------------------+-------------------------------+ |
| | Tarihsel ortalama vs şu an       | 6.520 vs 2.410 ort. (2003-24) | |
| | Tarihsel ortalamadan sapma       | +%170,5 (+2,3σ)               | |
| | Anomali durumu                   | Ortalamanın +2,3σ üstü        | |
| | En yoğun aylar (ilk 3)           | Ağu 2021 · Tem 2021 · Eyl 2020| |
| | Aktif sensörler                  | ○ MODIS (%28,4) ■ VIIRS (%71,6| |
| | En yoğun sensör                  | ■ VIIRS — 20.140 (%71,6)      | |
| | Son 7 gün tespit (NRT)           | 412 tespit (2026-11-08..15)   | |
| | Trend                            | Artış: +%18,4 (önceki dönem)  | |
| | VZA ve FRP belirsizlik (G9)      | 45,2 MW (olası aralık 38,7–51,8| |
| +----------------------------------+-------------------------------+ |
|                                                                      |
| ZAMAN SERİSİ (SON 12 AY / YILLIK)   --- Ham   ─── Harmonize  ⋮ 2012  |
| +------------------------------------------------------------------+ |
| |      /\        ..---..                                           | |
| |  .../  \..----/       \---..            /\                       | |
| | /                           \___...----/  \___                   | |
| +------------------------------------------------------------------+ |
|   Oca        Nis        Tem        Eki        Oca (UTC)              |
+----------------------------------------------------------------------+
| UYARI VE BELİRSİZLİK: Harmonize değerler model tahminidir, yer       |
| gerçeği değildir. FRP aralıkları ±1σ (~%68; uncertainty.z ile        |
| yapılandırılabilir) VZA, MODIS ölçek ve VIIRS öncesi k_c içerir.     |
| URL: https://pyrosync.example/?region=turkey&month=2021-08&mode=harm |
+======================================================================+
```

---

## 6. Türkiye / Akdeniz Demo Senaryosu (`S7` Verisi Hazır Olduğunda)

> **Gerçeklik Durumu:** `data/case-study/turkey/` dizini ve `S7` veri seti **henüz repoda mevcut değildir** (`S7` beklemede). Bu nedenle aşağıdaki tabloda kesinlikle uydurma sayı üretilmemiş, `S7` çıktısı geldiğinde doldurulacak yer tutucular kullanılmıştır.

**Hedef Senaryo:** 2021 Temmuz–Ağustos Antalya / Muğla mega yangın dönemi (`Türkiye / Akdeniz`, örnek bbox `27.3, 36.2, 32.6, 37.5` veya ulusal `turkey` seçili iken 2021-07 / 2021-08 dilimi).

| Brifing Öğesi | S7 Verisi İle Beklenen Gösterim Şablonu | Yer Tutucu / Veri Kaynağı |
|---|---|---|
| **Başlık ve Kapsam** | `Türkiye · Akdeniz (Antalya / Muğla) · Temmuz–Ağustos 2021 (UTC)` | `[S7_REGION_LABEL]` (`regions.json` veya `case-study/turkey`) |
| **Bulgu Cümlesi** | `Ham kayıt [S7_RAW_TOTAL] iken çoklu uydu çakışması ayıklandıktan sonra [S7_HARM_TOTAL] tekil tespit kaldı ([S7_REDUCTION_PCT]).` | `[S7_RAW_TOTAL]`, `[S7_HARM_TOTAL]`, `[S7_REDUCTION_PCT]` |
| **1. Tarihsel ort. vs şu an** | `[S7_AUG2021_HARM] vs [S7_AUG_BASE_MEAN] ort. (2003–2024)` | `[S7_AUG2021_HARM]`, `[S7_AUG_BASE_MEAN]` |
| **2. Ortalamadan sapma** | `+[S7_DELTA_PCT] (+[S7_Z_SCORE]σ)` | `[S7_DELTA_PCT]`, `[S7_Z_SCORE]` |
| **3. Anomali durumu** | `[■] Ortalamanın +[S7_Z_SCORE]σ üstü` | `--anom-pos-3` eşiği beklenir (`z >= +2,0`) |
| **4. En yoğun aylar** | `1. Ağu 2021 ([S7_AUG2021]) · 2. Tem 2021 ([S7_JUL2021]) · 3. [S7_3RD_MONTH]` | `[S7_AUG2021]`, `[S7_JUL2021]`, `[S7_3RD_MONTH]` |
| **5. Aktif sensörler** | `MODIS ([S7_MODIS_PCT]) · VIIRS ([S7_VIIRS_PCT])` (varsa NOAA-20 + Suomi NPP ayrımı) | `[S7_MODIS_PCT]`, `[S7_VIIRS_PCT]` |
| **6. En yoğun sensör** | `[S7_TOP_SENSOR] — [S7_TOP_SENSOR_COUNT] ([S7_TOP_SENSOR_PCT])` | `[S7_TOP_SENSOR]` |
| **7. Son 7 gün (NRT / Pik 7 gün penceresi)** | Tarihsel vaka incelemesinde *"Olay penceresi (7 gün): [S7_EVENT_START]–[S7_EVENT_END] → [S7_7D_COUNT] tespit"* biçiminde sabitlenir; canlı NRT seçilirse son 7 UTC gününü gösterir. | `[S7_7D_COUNT]` (`S7` olay penceresi) |
| **8. Trend** | Olay penceresi veya yıllık karşılaştırma: `Artış: +[S7_YOY_PCT] (2020 aynı aya göre)` | `[S7_YOY_PCT]` |
| **9. VZA ve G9 Belirsizlik** | `Ort. FRP: [S7_MEAN_FRP] MW (olası aralık [S7_FRP_LOWER]–[S7_FRP_UPPER])` | `[S7_MEAN_FRP]`, `[S7_FRP_LOWER]`, `[S7_FRP_UPPER]` |
| **Örnek Dışa Aktarma Dosyası** | `pyrosync_turkey_2021-08.csv` / `pyrosync_turkey_2021-08.geojson` (`_demo` eki yalnız demo kipindeyse) | — |

---

## 7. Tasarım Tokenları, Temalar ve Hareket Tercihi

### 7.1 Kullanılan Mevcut Tokenlar (`web/src/styles/tokens.css`)

- **Yüzey & Kenarlık:** `--color-surface`, `--color-surface-raised`, `--color-surface-sunken`, `--color-hover`, `--color-active`, `--color-border`, `--color-border-subtle`, `--color-border-strong`, `--color-focus`, `--border-width` (`1px`).
- **Metin:** `--color-text`, `--color-text-secondary`, `--color-text-muted`, `--color-text-disabled`, `--color-text-inverse`.
- **Durum & Sensör & Anomali:** `--color-status-demo` (demo etiketi nokta + metin), `--color-status-error` (hata çizgisi + metin), `--color-sensor-modis`, `--color-sensor-viirs`, `--color-duplicate-ring`, `--anom-neg-3`..`--anom-pos-3`, `--anom-none`.
- **Tipografi & Ölçüler:** `--font-sans`, `--font-mono`, `--font-size-2xs`..`--font-size-xl`, `--line-height-tight`/`snug`/`body`, `--space-half`..`--space-3`, `--radius-sm`/`md`/`pill`, `--shadow-raised`, `--shadow-drawer`, `--w-drawer` (`420px`), `--z-popover` (`40`), `--z-drawer` (`50`).
- **Tema Uyumu:** Tüm metrik kartları ve segmented control yukarıdaki semantik değişkenlere bağlandığı için varsayılan koyu tema ile `[data-theme="light"]` arasında ek renk kuralı gerektirmez. `--color-accent` burada **kullanılmaz**.
- **Hareket Tercihi (`prefers-reduced-motion`):** Çekmece açılış/kapanışı `--dur-drawer` (`240ms`) + `--ease-emphasized`, popover ve segmented seçici `--dur-ui` (`150ms`) + `--ease-standard` kullanır. `prefers-reduced-motion: reduce` etkinse tüm geçişler `0ms` veya `--dur-reduced` (`150ms` yalnızca saydamlık geçişi) olur; `preparing` yükleme çubuğu animasyon yerine statik desen gösterir.

### 7.2 Yeni Token Önerileri (`tokens.css`'e Eklenecek)

| Önerilen Token | Dark Değer | Light Değer | Print / Amaç |
|---|---|---|---|
| `--color-print-bg` | `#ffffff` | `#ffffff` | `@media print` A4 zorunlu beyaz zemin |
| `--color-print-text` | `#111111` | `#111111` | `@media print` yüksek kontrast siyah metin |
| `--color-print-muted` | `#444444` | `#444444` | `@media print` ikincil etiketler ve dipnot |
| `--color-print-border` | `#999999` | `#999999` | `@media print` tablo ve kart kenarlıkları |
| `--h-sparkline-print` | `64px` | `64px` | Yazdırma ve çekmece içi statik mini sparkline yüksekliği |

---

## 8. C6 "Bitti Sayılır" Kontrol Listesi ve R7 Devir (Handoff) Listesi

### 8.1 C6 "Bitti Sayılır" Kontrol Listesi

- [x] Export yerleşim kararı (`BottomBar` birincil popover + `BriefingDrawer` satır içi ayna) Fitts Yasası ve doygunluk bütçesiyle gerekçelendirildi.
- [x] `CSV` vs `GeoJSON` format seçici deseni (Segmented Control) ve Excel/QGIS tek satırlık açıklamaları EN/TR tanımlandı.
- [x] Dosya adı uyuşmazlığı (`pyrosync_{region}_{date}` vs sunucunun `pyrosync_{west}_{south}_{east}_{north}_{YYYYMMDD}.{ext}` çıktısı) belgelenip uzlaştırma + Görkem/R7 notu yazıldı.
- [x] 7 export durumu (`idle`, `preparing`, `success`, `error`, `empty`, `demo`, `offline`) masaüstü/mobil ve `aria` kurallarıyla tanımlandı.
- [x] `BriefingDrawer` koruma kararı, tetikleyici ve sıralı 9 metrik (EN/TR, formül, veri kaynağı, `UX_YAZIM.md` biçimi, G9 belirsizlik bandı) çıkarıldı; bugün hesaplanabilir 7 metrik açıkça işaretlendi.
- [x] `1366×768` masaüstü, `375×812` mobil ve `boş`/`üretiliyor`/`hazır`/`hata` wireframe'leri çizildi.
- [x] `@media print` A4 dikey tek sayfa kuralları, S/B okunabilirlik, sayfa kırılımı ve A4 wireframe tamamlandı.
- [x] Türkiye/Akdeniz (`S7`) senaryosu sayı uydurulmadan yer tutucularla belgelendi.

### 8.2 R7 Devir (Handoff) Listesi

#### Bileşenler ve Kancalar (Components & Hooks)
1. **`web/src/components/Export/ExportPopover.tsx` (yeni) + `ExportFormatSwitch.tsx` (yeni):** `BottomBar` ("Bu görünümü indir") popover'ı ve `BriefingDrawer` altındaki satır içi `CSV | GeoJSON` segmented control bileşenini ortaklaştırmak. Kapsam, bbox, satır önizlemesi, `truncated` uyarısı ve `preparing`/`error`/`empty`/`demo`/`offline` durumlarını yönetmek.
2. **`web/src/hooks/useExportView.ts` + `web/src/lib/csv.ts` + `web/src/lib/geojson.ts` (yeni):**
   - Arşiv görünümü için istemci tarafı GeoJSON üreteci (`FeatureCollection`, WGS84 `[lon, lat]`) eklemek; arşiv CSV sütunlarına `vza_factor`, `frp_lower_mw`, `frp_upper_mw` eklemek (`WireHarmonizedPoint` güncellendikten sonra).
   - NRT aktifken `/api/export?west=...&south=...&east=...&north=...&start=...&end=...&format=...` indirmesini `fetch` + `Blob` ile yöneterek `preparing`/`success`/`empty`/`error` durumlarını yakalamak.
   - `csvFileName()` / `exportFileName()` yardımcılarını bölüm 1.4'teki birleşik adlandırmaya (`_demo` eki dahil) güncellemek.
3. **`web/src/components/Drawers/BriefingDrawer.tsx` + `.module.css` + `Drawer.module.css`:**
   - Metrik listesini Bölüm 3.2'deki 1–9 sırasına genişletmek; `boş`, `üretiliyor` (iskelet kartlar), `hazır`, `hata` durumlarını desteklemek.
   - Statik SVG mini sparkline bileşeni (`BriefingSparkline.tsx`) eklemek.
   - Mobilde (`< 640px`) tam yükseklik çekmece sınıfı ve sabit alt eylem çubuğu (Segmented Format + İndir + Yazdır + Bağlantıyı Kopyala) eklemek.
   - Bölüm 5.1'deki `@page` + `@media print` kurallarını (`color-scheme: light`, zorunlu açık palet, S/B desenler, baskı üst/alt bilgi bloğu, `break-inside: avoid`) uygulamak.
4. **`web/src/hooks/useRegionData.ts` + `web/src/services/api.ts` + `web/src/types/index.ts`:**
   - `WireHarmonizedPoint` ve `WireCalendarCell` üzerine backend'in halihazırda ürettiği `frp_lower`, `frp_upper`, `mean_frp_lower`, `mean_frp_upper`, `total_frp`, `total_frp_lower`, `total_frp_upper`, `frp_per_detection_eq` ve `uncertainty` alanlarını eşlemek (`HarmonizedHotspot` üzerinde `calibration_ratio` artık yoktur).
   - `useBriefingMetrics()` türetici kancasını yazarak Bölüm 3.2'deki 8+ metriği (`top3Months`, `dominantSensor`, `baselineComparison`, `computeBriefingTrend`, G9 aralık biçimlendirmesi) hesaplamak.

#### Eklenecek `web/src/content/copy.ts` Anahtarları (EN / TR Çiftleri)
- **Export kontrolü & açıklamalar:**
  - `exportTrigger`: `"Download this view"` / `"Bu görünümü indir"`
  - `exportFormatCsvHelp`: `"Point table for Excel, LibreOffice, and spreadsheets (UTF-8)."` / `"Excel, LibreOffice ve tablo araçları için nokta listesi (UTF-8)."`
  - `exportFormatGeojsonHelp`: `"WGS84 spatial layer for QGIS, ArcGIS, and GIS tools."` / `"QGIS, ArcGIS ve CBS araçları için WGS84 konum katmanı."`
  - `exportPreviewRows`: `"{count} records · approx. {size}"` / `"{count} kayıt · yaklaşık {size}"`
  - `exportTruncatedNote`: `"Map view reached the {limit}-point limit; download covers the visible subset."` / `"Harita görünümü {limit} nokta sınırına ulaştı; indirilen dosya görünür alt kümeyi kapsar."`
- **Export durumları:**
  - `exportPreparing`, `exportSuccess`, `exportError`, `exportRetry`, `exportEmpty`, `exportDemoNote`, `exportOffline` (tam EN/TR metinleri Bölüm 2'den birebir alınır).
- **Brifing metrik etiketleri ve durumları:**
  - `briefingMetricBaselineVsNow`, `briefingMetricDeviation`, `briefingMetricAnomalyStatus`, `briefingMetricPeakMonths`, `briefingMetricActiveSensors`, `briefingMetricDominantSensor`, `briefingMetricLast7Days`, `briefingMetricTrend`, `briefingMetricUncertaintyRange`, `briefingLikelyRange` (`"likely {low}–{high}"` / `"olası aralık {low}–{high}"`), `briefingStateEmpty`, `briefingStatePreparing`, `briefingStateError`, `briefingPrintFooterDisclaimer`, `briefingGeneratedAt`.

#### API Çağrıları ve Backend Koordinasyon Notları
- **Mevcut API'ler:**
  - Arşiv serisi & noktalar: `GET /api/regions`, `GET /api/series?region=...`, `GET /api/points?region=...&year=...&month=...`
  - Canlı NRT & Export: `GET /api/stats`, `GET /api/harmonize`, `GET /api/export`
- **Backend koordinasyon gereksinimleri (Görkem / G9 / G10):**
  1. `GET /api/export`: İsteğe bağlı `region` slug parametresi ile okunabilir dosya adı (`pyrosync_{region}_{scope}_{YYYYMMDD}.{ext}`) + CSV UTF-8 BOM + cache HIT tarih kararlılığı.
  2. `Son 7 gün` NRT sınırı: `crates/server/src/main.rs` içindeki `parse_time_window` bugün `days: 1..=5` kabul ediyor (`DayRange::new`). R7'nin 7 gün gösterebilmesi için `GET /api/stats` ve `GET /api/export` çağrıları `start` & `end` (7 günlük UTC aralık) parametreleriyle yapılmalı veya `DayRange` üst sınırı `7`'ye çıkarılmalıdır.
  3. Arşiv G9 alanları: `archive_api.rs` `MonthEntry` yanıtına ileride aylık ortalama FRP alt/üst sınırları eklenirse arşiv brifinginde de doğrudan G9 MW bandı gösterilebilir.
