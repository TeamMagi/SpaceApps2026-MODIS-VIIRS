# PyroSync — UI/UX Konsept ve Wireframe (C1)

## 1. Tasarım İlkeleri

1. **30 Saniyelik Jüri Kavrayışı (Tek Aksiyonla Kanıt):** Kullanıcı uygulamayı açtığı anda hazır/varsayılan bölge verisi üzerinde **Raw ↔ Harmonised** geçişini tek tıkla deneyebilmelidir. İlk 30 saniyede "ham MODIS+VIIRS verisinde çift sayım ve 2012 Sensor Cliff sıçraması var → harmonizasyon bunu düzeltiyor" tezi ek açıklamaya ihtiyaç duymadan anlaşılmalıdır.
2. **Çift Kanıt İlkesi (Mekân + Zaman Senkronizasyonu):** Uydu harmonizasyonunun etkisi yalnızca özet sayılarla değil, aynı anda **harita** (mekânsal çift sayımın ve FRP şişkinliğinin azalması) ve **zaman serisi / yanma takvimi** (2012 sonrası yapay artışın sönümlenmesi) üzerinde eşzamanlı gösterilir.
3. **Kaynak ve Ölçüm Şeffaflığı:** Her hotspot noktası ve istatistik kartı, arkasındaki sensör kökenini (`ModisOnly`, `ViirsOnly`, `Matched`), ham FRP (`frp`) ile düzeltilmiş FRP (`frp_corrected`) arasındaki farkı ve veri modunu (canlı `/api/harmonize` yanıtı veya örnek veri fallback'i) açıkça görünür kılar.
4. **Kademeli Derinleşme (Özet Üstte, Detay İstek Üzerine):** İlk bakışta yalnızca karar verici katman (Toggle, 4 KPI kartı, Harita, Sensor Cliff Grafiği ve Takvim) sunulur. Nokta bazlı koordinat/FRP kırılımı ve aylık anomali metrikleri harita popover'ı ve takvim tooltip'i ile talep üzerine açılır.
5. **Bilimsel Araçtan Taşınabilir Sahaya (Mobile-First):** Masaüstünde paralel analize izin veren çok panelli düzen, mobil cihazlarda öncelik sırasına göre (Toggle → Özet Sayı → Harita → Grafik/Takvim sekmeleri) yeniden akar; hiçbir kritik karşılaştırma yatay kaydırmanın arkasına gizlenmez.

---

## 2. Ana Sayfa Layout'u (Desktop)

Masaüstü görünümü, **üstte sabit kontrol + kanıt şeridi**, **altta iki sütunlu asimetrik analiz alanı** prensibiyle kurgulanır:

- **Üst Başlık (`AppShell`):** Sol tarafta uygulama adı ve amacı; sağ tarafta API sağlık/veri kaynağı göstergesi (`getHealth`) ve hızlı jüri demo senaryosu seçicisi.
- **Komuta ve Geçiş Şeridi:** Solda bölge (`west, south, east, north`) ve zaman aralığı (`days`) kontrolleri; sağda ekranın birincil odak noktası olan **`ViewToggle` (`Raw` ↔ `Harmonised`)**.
- **İstatistik Şeridi (`StatsBar`):** 4 eşit genişlikli KPI kartı; mevcut `raw` vs `harmonized` farkını ve takvim anomali özetini taşır.
- **Ana Çalışma Alanı:**
  - **Sol Panel (~%58 genişlik):** İnteraktif harita (`FireMap`). Aktif mod lejandı ve takvimden gelen ay filtresi haritanın üst kenarında taşınır.
  - **Sağ Panel (~%42 genişlik, dikey iki blok):** Üstte **Sensor Cliff Zaman Serisi Grafiği** (Recharts), altta **Burning Activity Calendar** (Yıl × Ay ısı matrisi).

```text
+----------------------------------------------------------------------------------------+
| PyroSync - MODIS/VIIRS Harmonizasyon          [Durum: Canli API]  [Senaryo: Akdeniz v] |
+----------------------------------------------------------------------------------------+
| Bolge: [26.0, 36.0, 36.0, 42.0]  Donem: [2000-2024 v]  [Getir] | [ Raw | *HARMONISED* ] |
+----------------------------------------------------------------------------------------+
| +------------------+  +------------------+  +------------------+  +------------------+ |
| | Aktif Tespit     |  | Elenen Tekrar    |  | Ort. FRP Etkisi  |  | Takvim & Anomali | |
| | 892 tespit       |  | -355 (%28.5)     |  | 42.1 -> 35.8 MW  |  | 180 ay | +2.4z   | |
| | Ham: 1,247       |  | Interscan+Esles. |  | VZA duzeltmeli   |  | Pik: Agu 2021    | |
| +------------------+  +------------------+  +------------------+  +------------------+ |
+---------------------------------------------------+------------------------------------+
| HARITA: Mekansal Yangin Yogunlugu                 | GRAFIK: Sensor Cliff Zaman Serisi  |
| [Filtre: Tum Aylar]     [Lejant: Tekil/Birlesik]  |  FRP/Sayi     2012 (VIIRS)         |
|                                                   |   ^           :   .. .. .. (Raw)   |
|     .   *       o   @         .     *             |   |    _  _   :  _--_--_-- (Harm.) |
|       @   @   *       .   o       @   @           |   |___/ \/ \__:_/                  |
|     *   @ @   o       .       @   *   .           |   +-----------+-+----------------> |
|         o   *     [  +  ]         .   o           |   2000       2012             2024 |
|                 (Secili Alan)                     +------------------------------------+
|                                                   | TAKVIM: Burning Activity Calendar  |
|   [-] [+] [BBox Yenile]      [Katman: Harmonized] |       O S M N M H T A E E K A      |
|                                                   | 2021  . . . : : # @ @ # : . .      |
|                                                   | 2022  . . : : # # @ # : . . .      |
|                                                   | 2023  . . . : # @ @ # # : . .      |
|                                                   | 2024  . . . : : # @ * : . . -      |
|                                                   | Anomali: [.] Dusuk [:] Ort [@] Pik |
+---------------------------------------------------+------------------------------------+
```

---

## 3. Kullanıcı Akışı

Temel kullanıcı yolculuğu 4 ardışık fazdan oluşur: **Bölge Seçimi → Veri Yükleme → Raw / Harmonised Geçişi → Detay İnceleme**.

| Aşama | Kullanıcı Eylemi | Tetiklenen API / Veri Katmanı | Ekranda Değişen Öğeler |
| :--- | :--- | :--- | :--- |
| **1. İlk Açılış & Hazır Demo** | Uygulama açılır veya senaryo menüsünden hazır bir bölge (ör. Akdeniz, Sibirya, Kaliforniya) seçilir. | Açılışta `services/api.ts` üzerinden `getHealth()` (`GET /api/health`) çağrılır. Varsayılan başlangıç için `SAMPLE_RESPONSE` veya varsayılan bbox ile `getHarmonized(...)` yüklenir. | `AppShell` başlığında bağlantı durumu güncellenir; harita ilgili bbox sınırlarına odaklanır; `ViewToggle` varsayılan olarak `"harmonized"` konumunda açılır. |
| **2. Bölge & Dönem Seçimi** | Kullanıcı hazır bölge seçer, haritada bbox çizer/günceller veya `days` penceresini değiştirip "Getir"e basar. | `hooks/useApi.ts` üzerinden `getHarmonized(west, south, east, north, days)` (`GET /api/harmonize?west=...&south=...&east=...&north=...&days=...`) tetiklenir. | Önceki seçili nokta (`selectedHotspot`) ve aktif ay filtresi (`selectedMonth`) sıfırlanır; yükleme durumuna geçilir. |
| **3. Raw ↔ Harmonised Geçişi** | Kullanıcı üst şeritteki `ViewToggle` üzerinden `"raw"` ve `"harmonized"` arasında geçiş yapar. | **Yeni API çağrısı yapılmaz.** Tek `HarmonizeResponse` yükü içindeki `raw`, `harmonized` ve `calendar` verileri istemci belleğinde anlık yeniden yorumlanır. | Harita nokta katmanı, `StatsBar` değerleri ve `SensorCliffChart` serisi eşzamanlı güncellenir; mod rozeti değişir. |
| **4. Detay İnceleme (Harita & Takvim)** | Kullanıcı haritada bir hotspot noktasına tıklar veya takvimde bir `year × month` hücresine tıklar/üzerine gelir. | **Yeni API çağrısı yapılmaz.** `harmonized.hotspots` ve `calendar` dizileri istemci tarafında filtrelenir. | Harita noktasına tıklanınca `HotspotPopover` açılır. Takvim hücresine tıklanınca `selectedMonth` set edilir ve harita yalnızca o ayın `acquired` tarihli hotspot'larını vurgular. |

### Sistem Durumları (Loading / Empty / Error)

- **Loading State (Yükleniyor):**
  - `StatsBar` kartları sabit yükseklikte iskelet (skeleton) yer tutuculara döner (layout kayması olmaz).
  - Harita ve sağ paneller üzerinde yarı saydam bir yükleme katmanı belirir: *"FIRMS uydu verileri çekiliyor ve harmonizasyon uygulanıyor..."*
  - `ViewToggle` ve "Getir" butonu istek tamamlanana kadar kilitlenir (`disabled`), çift istek engellenir.
- **Empty State (Veri Yok):**
  - `data.raw.total_raw === 0 && data.harmonized.count === 0` durumunda harita seçili bbox çerçevesini gösterir ancak nokta basmaz.
  - İstatistik kartları `0` değerini gösterir; harita ve takvim alanının merkezinde *"Seçili koordinat ve zaman aralığında aktif yangın tespiti bulunamadı"* mesajı ile iki hızlı aksiyon sunulur: **[Zaman Aralığını Genişlet]** ve **[Örnek Bölge Yükle]**.
- **Error State (Hata):**
  - `useApi` içindeki `error` alanı dolduğunda (ör. `FIRMS_API_KEY` eksikliği, ağ hatası veya timeout), üst alanda kapatılamayan ancak aksiyon alınabilen bir `DataStateBanner` açılır.
  - Banner'da teknik hata özeti, **[Tekrar Dene]** butonu ve jüri demosunun kesintiye uğramaması için **[Örnek Veri ile Devam Et (`SAMPLE_RESPONSE`)]** kurtarma seçeneği yer alır.

---

## 4. Ekran Bazlı Wireframe'ler

### (a) Ana Sayfa / Dashboard — Raw Mod Aktif Görünümü

Ana dashboard'un Raw modundaki görünümü (Harmonised görünüm Bölüm 2'de verilmiştir). Raw modda üst şerit uyarı vurgusu taşır, `StatsBar` çift sayım riskini öne çıkarır ve sağdaki grafikte 2012 sıçraması belirginleşir.

```text
+----------------------------------------------------------------------------------------+
| PyroSync - MODIS/VIIRS Harmonizasyon          [Durum: Canli API]  [Senaryo: Akdeniz v] |
+----------------------------------------------------------------------------------------+
| Bolge: [26.0, 36.0, 36.0, 42.0]  Donem: [2000-2024 v]  [Getir] | [*RAW* | Harmonised ] |
+----------------------------------------------------------------------------------------+
| +------------------+  +------------------+  +------------------+  +------------------+ |
| | Ham Tespit (Raw) |  | MODIS Payi       |  | VIIRS Payi       |  | Cift Sayim Riski | |
| | 1,247 tespit     |  | 412 tespit       |  | 835 tespit       |  | +355 fazla kayit | |
| | Filtresiz toplam |  | VZA duzeltmesiz  |  | Coklu uydu cakis.|  | Duzeltilmemis!   | |
| +------------------+  +------------------+  +------------------+  +------------------+ |
+---------------------------------------------------+------------------------------------+
| HARITA: Ham Sensor Tespitleri (MODIS vs VIIRS)    | GRAFIK: Ham Sensor Cliff Etkisi    |
| [Mod: RAW]    [Lejant: (M) MODIS  (V) VIIRS]      |  Tespit       2012 (VIIRS)         |
|                                                   |   ^           :     /^^^^\  (Raw)  |
|   M V M V   V V     V M V       M V   V           |   |           :    /      \        |
|     V M V V   M   V   M V V   V M V M             |   |   _  _    :___/                |
|   M V   V M   V V   M   V M   V   V M             |   +-----------+-+----------------> |
|     V M   V     V M   V     M V   V               |   2000       2012             2024 |
|                                                   +------------------------------------+
|   [!] Ayni yangin birden fazla kez sayiliyor      | TAKVIM: Ham Aylik Yogunluk         |
|   [-] [+] [BBox Yenile]         [Katman: Raw]     |       O S M N M H T A E E K A      |
|                                                   | 2021  . . : : # @ @ @ # : . .      |
|                                                   | 2022  . . : # # @ @ @ # : . .      |
|                                                   | (2012 sonrasi tum hucreler sisik)  |
+---------------------------------------------------+------------------------------------+
```

### (b) Harita Görünümü + Hotspot Detay Popover'ı

Harita üzerinde yakınlaştırılmış durum, kümeleme (cluster) davranışı ve seçili bir hotspot tıklandığında açılan `HotspotPopover` detay kartı:

```text
+----------------------------------------------------------------------------------------+
| HARITA GORUNUMU: Akdeniz Bolgesi (Zoom: 8)      [Aktif Ay: Agustos 2023 (Temizle x)]   |
+----------------------------------------------------------------------------------------+
| [Katman: Harmonised]  [Boyut: frp_corrected]  [Guven Filtresi: Tumu v]                 |
|                                                                                        |
|       .   *               ( 14 )  <-- Kume (Cluster)                                   |
|                          /      \                                                      |
|     o       @     *     o        *        +----------------------------------------+   |
|                 @                         | HOTSPOT DETAYI               [Kapat x] |   |
|           o   [(@)] <---------------------+----------------------------------------|   |
|                 |    Secili Nokta         | Konum     : 36.8842 N, 30.7056 E       |   |
|       *         |                         | Tarih     : 2023-08-14 11:42 (Day)     |   |
|                 o         .       *       | Kaynak    : Matched (MODIS + VIIRS)    |   |
|                                           | Guven     : High                       |   |
|     .       ( 7 )             o           |----------------------------------------|   |
|                                           | Ham FRP   : 68.4 MW                    |   |
|                                           | Harm. FRP : 54.1 MW (-%20.9 VZA duz.)  |   |
|                                           +----------------------------------------+   |
|                                                                                        |
| [-] [+] [BBox'a Sigdir]   Lejant: [.] Dusuk FRP  [*] Orta FRP  [@] Yuksek FRP          |
+----------------------------------------------------------------------------------------+
```

- **Etkileşim kuralları:**
  - Küme balonuna (`( 14 )`) tıklanınca harita o kümenin sınırlarına otomatik yakınlaşır.
  - Popover içinde `source` değeri (`ModisOnly`, `ViirsOnly`, `Matched`) açıkça gösterilir; böylece kullanıcının "bu nokta iki uydudan eşleştirilerek tekilleştirildi" bilgisini nokta düzeyinde doğrulaması sağlanır.
  - `frp` ve `frp_corrected` yan yana/alt alta verilerek VZA ve kalibrasyon düzeltmesinin tek piksel üzerindeki etkisi kanıtlanır.

### (c) Burning Activity Calendar Görünümü (Yıl × Ay Isı Matrisi)

`CalendarCell[]` dizisini `year` (satır) × `month` (sütun, 1–12) matrisi olarak sunar. Hücreler birincil olarak `anomaly` değerine göre kademelenir; `anomaly === null` ise baseline yetersizliği olarak özel nötr/çizgili durumla ayrılır.

```text
+----------------------------------------------------------------------------------------+
| BURNING ACTIVITY CALENDAR (2018 - 2024)       [Metrik: Anomali (z) | Tespit | Ort FRP] |
+----------------------------------------------------------------------------------------+
|          Oca  Sub  Mar  Nis  May  Haz  Tem  Agu  Eyl  Eki  Kas  Ara  | Yillik Toplam   |
|  2018  |  . |  . |  . |  : |  : |  # |  # |  @ |  : |  . |  . |  . | |   412 tespit    |
|  2019  |  . |  . |  : |  : |  # |  # |  @ |  # |  : |  . |  . |  . | |   448 tespit    |
|  2020  |  . |  . |  . |  : |  : |  # |  @ |  @ |  # |  : |  . |  . | |   503 tespit    |
|  2021  |  . |  . |  . |  : |  # |  @ | [*] | @ |  # |  : |  . |  . | |   789 tespit    |
|  2022  |  . |  . |  . |  : |  : |  # |  # |  @ |  : |  . |  . |  . | |   430 tespit    |
|  2023  |  . |  - |  . |  : |  # |  # |  @ | [@]|  # |  : |  . |  . | |   615 tespit    |
|  2024  |  . |  . |  . |  : |  # |  @ |  @ |  # |  - |  - |  - |  - | |   390 tespit    |
+----------------------------------------------------------------------------------------+
|  HOVER TOOLTIP (Tem 2021):                       SECILI AY FILTRESI:                   |
|  +-------------------------------------------+   [Temmuz 2021 secili -> Haritada 198   |
|  | Donem   : Temmuz 2021                     |    nokta vurgulaniyor | Filtreyi Kaldir]|
|  | Tespit  : 198 harmonize yangin            +-----------------------------------------+
|  | Ort FRP : 61.4 MW                         | Anomali Skalasi:                        |
|  | Anomali : +3.18 sigma (Ekstrem Sezon)     | [.] <= 0  [:] 0..1  [#] 1..2  [@] > 2   |
|  | Tikla   -> Haritayi bu aya filtrele       | [-] Veri / Baseline yok (anomaly: null) |
|  +-------------------------------------------+                                         |
+----------------------------------------------------------------------------------------+
```

### (d) Raw ↔ Harmonised Karşılaştırma Paneli + Sensor Cliff Zaman Serisi Grafiği

Jüri sunumunun en kritik bileşeni olan `ViewToggle` merkezli karşılaştırma paneli ve **Sensor Cliff** grafiği:

```text
+----------------------------------------------------------------------------------------+
| RAW <-> HARMONISED KARSILASTIRMA PANELI              Aktif Mod: [ Raw | *HARMONISED* ]  |
+----------------------------------------------------------------------------------------+
| +-----------------------------------------+  +---------------------------------------+ |
| | SENSOR KAYNAK DAGILIMI (Harmonize Etki) |  | FARK OZETI                            | |
| | MODIS Only  [========      ]  198       |  | Ham Toplam   : 1,247 tespit           | |
| | VIIRS Only  [============= ]  512       |  | Harmonize    :   892 tespit           | |
| | Matched     [======        ]  182       |  | Cift Sayim   :  -355 (%28.5 elendi)   | |
| | (Matched = MODIS+VIIRS tekillestirildi) |  | 2012 Sicrama : Normalize edildi       | |
| +-----------------------------------------+  +---------------------------------------+ |
+----------------------------------------------------------------------------------------+
| ZAMAN SERISI: Yillik Yangin Aktivitesi & 2012 Sensor Cliff                             |
|                                                                                        |
|  Tespit / Yil                                                                          |
|   900 |                                   2012: VIIRS Devreye Giris                    |
|       |                                      |  ....Raw (MODIS+VIIRS toplam)           |
|   700 |                                      | .    ...   ....   ...                   |
|       |   [Bolge A: Yalniz MODIS]            |: [Yapay Sensor Sicramasi]               |
|   500 |                                      |                                         |
|       |       /\       /\                    |  ____    ____    ____   ____            |
|   300 |  ____/  \_____/  \____               | /    \__/    \__/    \_/    \ Harmonised|
|       |                       \______________|/   (Surekli, normalize trend)           |
|   100 |                                      |                                         |
|     0 +--------------------------------------+---------------------------------------> |
|      2000   2004   2008   2010   2012      2015      2018      2021      2024          |
+----------------------------------------------------------------------------------------+
```

---

## 5. Raw ↔ Harmonised Toggle'ın Tasarım Davranışı

### Yerleşim ve Geçiş Modeli Kararı: Yerinde Senkronize Cross-Fade

Bu geçiş için üç alternatif değerlendirilmiştir:
- *Yan yana (Split View):* Haritayı ve grafiği ikiye böldüğü için her paneli daraltır; özellikle mobilde kullanılamaz hale gelir ve kullanıcının gözünü iki ayrı koordinat sistemi arasında gidip gelmeye zorlar.
- *Üst üste kaydırma (Before/After Slider):* Yalnızca harita üzerinde çalışır; `StatsBar` ve Recharts zaman serisinde tutarlı bir etkileşim üretmez.
- **Seçilen Model — Yerinde (In-Place) Anlık Geçiş + Kısa Cross-Fade:** Tüm bileşenler aynı konumu korur. Kullanıcı gözünü haritadaki tek bir yangın kümesine veya grafikteki 2012 çizgisine sabitlemişken toggle'a basar; fazla noktaların sönerek kaybolduğunu, sayaçların düştüğünü ve sıçramanın düzleştiğini **aynı piksel koordinatlarında** görür.

### Aynı Anda Değişen 3 Katman

1. **Harita Nokta Yoğunluğu ve Kaynak Kodlaması (`FireMap`):**
   - **Raw modda:** Noktalar sensör kaynaklarına göre ayrışır (MODIS vs VIIRS ayrı işaretlenir), eşleşmiş (`Matched`) bölgelerde çakışan ham noktalar ve ham `frp` büyüklükleri görünür.
   - **Harmonised modda:** Tekilleştirilmiş `harmonized.hotspots` kümesi gösterilir; tekrar eden tespitler fade-out ile kaybolur, kalan noktalar birleşik ölçekte `frp_corrected` büyüklüğüne göre yeniden boyutlanır.
   - *Veri sözleşmesi notu:* Mevcut `HarmonizeResponse` şemasında `raw` alanı yalnızca toplam sayaçları (`modis_count`, `viirs_count`, `total_raw`) dönerken nokta dizisi `harmonized.hotspots` altındadır. Bu nedenle ilk aşamada Raw harita görünümü, `HarmonizedHotspot.source` (`ModisOnly`, `ViirsOnly`, `Matched`) ve ham `frp` alanları üzerinden sensör ayrıştırılarak simüle edilir; backend ham nokta listesi eklerse doğrudan o diziye bağlanır.
2. **İstatistik Sayıları (`StatsBar`):**
   - **Raw modda:** Birincil metrik `raw.total_raw` olur; yanında `raw.modis_count`, `raw.viirs_count` ve "Çift Sayım Riski" vurgusu görünür.
   - **Harmonised modda:** Birincil metrik `harmonized.count` olur; yanında `duplicatesRemoved` (`total_raw - harmonized.count` ve `%` azalma) ile ham `frp` → `frp_corrected` ortalama düzeltme farkı gösterilir.
3. **Zaman Serisi Grafiği (`SensorCliffChart`):**
   - **Raw modda:** Ham tespit/FRP eğrisi ön plandadır ve 2012 sonrasında belirgin bir yukarı basamak (Sensor Cliff) çizer.
   - **Harmonised modda:** Harmonize eğri ön plana gelir ve 2012 çizgisini kesintisiz geçer; Raw eğrisi ise arkada soluk ve kesikli bir "hayalet referans çizgisi" olarak bırakılır. Böylece kullanıcı her iki eğri arasındaki boşluğu (temizlenen yapay artışı) doğrudan ölçebilir.

### Kullanıcının Farkı Algılama Mekanizması

- `ViewToggle` üzerindeki aktif durum etiketi ve yüksek kontrastlı aktif sekme vurgusu,
- `StatsBar` içindeki doğrudan delta metni (ör. `-355 (%28.5) tekrar elendi`),
- Grafikteki durum rozetinin `"Yapay Sıçrama Görünür"` → `"2012 Geçişi Normalize Edildi"` olarak değişmesi,
- Harita lejandının `"MODIS / VIIRS Ayrımı"` ↔ `"Tekilleştirilmiş Harmonize Veri"` şeklinde güncellenmesi.

---

## 6. Sensor Cliff Görselleştirmesi

2012 yılında VIIRS (S-NPP) uydusunun devreye girmesiyle aynı alandaki günlük gözlem sayısı ve küçük yangın hassasiyeti artar; bu da gerçek yangın artışı yokken zaman serisinde yapay bir sıçrama (**Sensor Cliff**) üretir.

### Grafik İşaretleme Anatomisi (Recharts)

1. **Dikey Referans Çizgisi (`ReferenceLine`):** X ekseninde `2012` noktasında tam yükseklikte kesikli dikey çizgi yer alır.
2. **Tepe Etiketi (Annotation Badge):** Çizginin üst ucunda `"2012: VIIRS Başlangıcı (Sensor Cliff)"` etiketi bulunur.
3. **Dönemsel Arka Plan Bölgesi (`ReferenceArea`):**
   - `2000–2011`: *"Yalnız MODIS Dönemi (Baseline)"*
   - `2012+`: *"Çoklu Sensör Dönemi (MODIS + VIIRS)"* — hafif gölgeli arka plan ile ayrılır.
4. **İkincil Bilgi Notu:** 2017 ve sonrası ek VIIRS uyduları için ana çizgiyi kalabalıklaştırmamak adına grafiğin üst/alt bilgi satırında veya hover tooltip'inde uydu sayısı notu gösterilir.

### Raw vs Harmonised Davranışı

- **Raw Modda:** 2012 dikey çizgisinin hemen sağında seri dik bir basamak yapar. Sıçrama bölgesinin içinde yönlü bir çağrı etiketi (*"Yapay sensör sıçraması — gerçek yangın artışı değil"*) görünür.
- **Harmonised Modda:** Aynı 2012 çizgisi ve gölgeli dönem sabit kalır; ancak ana eğri 2012 sınırını kırılma olmadan geçer. Raw serisi kesikli hayalet çizgi olarak arkada tutulur; iki çizgi arasındaki alanın küçülmesi/kapanması harmonizasyonun başarısını görsel olarak kanıtlar.

---

## 7. Mobil Responsive Davranış

Jüri üyeleri uygulamayı telefonlarından açabileceğinden, tüm arayüz **mobile-first** yaklaşımla tasarlanır.

### Breakpoint Stratejisi

| Breakpoint | Ekran Sınıfı | Layout ve Yeniden Akış Davranışı |
| :--- | :--- | :--- |
| **`< 640px` (Varsayılan)** | Mobil | Tek sütun dikey düzen. `AppShell` başlığı tek satıra sığdırılır. `ViewToggle` üstte tam genişlikli ve yapışkan (sticky) kalır. Bölge/Tarih filtreleri tek bir özet çipine (`Akdeniz · Son 30g [Düzenle]`) sıkıştırılır; tıklandığında alttan açılan çekmece (Bottom Sheet) içinde açılır. `StatsBar` 2×2 kompakt ızgara olur. Harita (`40vh`) üstte durur; altında **Karşılaştırma & Sensor Cliff** ile **Burning Calendar** arasında geçiş yapan 2'li sekme çubuğu yer alır (uzun dikey scroll engellenir). Hotspot detayı popover yerine alttan açılan Bottom Sheet'tir. |
| **`640px – 1023px`** | Tablet | `StatsBar` 4'lü tek satır veya 2×2 geniş kartlara geçer. Bölge/Tarih kontrolleri tek satırda görünür. Harita tam genişlik (`45vh`) alır; altında Grafik ve Takvim yan yana 2 sütun olarak yerleşir. |
| **`>= 1024px`** | Desktop | Bölüm 2'deki asimetrik 2 sütunlu komuta ekranı (`~58%` Harita sol, `~42%` Grafik + Takvim sağ), hotspot detayları nokta yanında floating `HotspotPopover`. |

### Mobil Wireframe (`< 640px`, 42 karakter genişlik)

```text
+----------------------------------------+
| PyroSync      [Canli]  [Senaryo: Akd v]|
+----------------------------------------+
| Bolge: Akdeniz | 2000-2024  [Filtrele] |
+----------------------------------------+
| [   Raw Data   |    *HARMONISED*     ] |
+----------------------------------------+
| +----------------+  +----------------+ |
| | Tespit: 892    |  | Elenen: -355   | |
| | Ham: 1,247     |  | %28.5 azalma   | |
| +----------------+  +----------------+ |
| +----------------+  +----------------+ |
| | Ort FRP: 35.8  |  | Anomali: +2.4z | |
| +----------------+  +----------------+ |
+----------------------------------------+
| HARITA (Dokunmatik, 40vh)              |
|   .   *       o   @       .   *        |
|     @   @   *     .   o     @          |
|         o     (@) <- Tikla             |
| [-] [+] [Yenile]     [Katman: Harm.]   |
+----------------------------------------+
| [ *Sensor Cliff* |  Yanma Takvimi    ] |
+----------------------------------------+
|  2012 (VIIRS)                          |
|   ^       :  .. .. .. (Raw)            |
|   |  _  _ : _--_--_-- (Harm.)          |
|   +-------+-+----------------------->  |
|  2000   2012                     2024  |
+----------------------------------------+
| (Noktaya tiklaninca Bottom Sheet acilir|
|  Ham FRP: 68.4 -> Harm. FRP: 54.1 MW)  |
+----------------------------------------+
```

---

## 8. Bileşen Ağacı

Mevcut `web/src/` kurallarına (her bileşen `components/FeatureName/FeatureName.tsx`, API çağrıları `services/api.ts`, paylaşılan tipler `types/index.ts`, grafikler `components/Charts/`) uygun hiyerarşi:

```text
web/src/
├── App.tsx                              [Kök koordinasyon state'i]
├── types/index.ts                       [HarmonizeResponse, HarmonizedHotspot, CalendarCell, ViewMode vb.]
├── services/api.ts                      [getHealth, getHarmonized]
├── hooks/
│   ├── useApi.ts                        [Genel async istek state'i (mevcut)]
│   └── useHarmonizeData.ts              [BBox + days ile veri çekme ve fallback yönetimi]
├── mocks/sample-data.ts                 [SAMPLE_RESPONSE (mevcut)]
└── components/
    ├── Layout/AppShell.tsx              [Sayfa iskeleti, başlık, durum & senaryo yuvası (mevcut)]
    ├── Controls/FilterBar.tsx           [BBox / demo senaryo + days seçici + Getir]
    ├── Toggle/ViewToggle.tsx            [Raw <-> Harmonised geçiş butonu (mevcut)]
    ├── Status/DataStateBanner.tsx       [Loading / Empty / Error + Örnek Veri kurtarma]
    ├── Stats/StatsBar.tsx               [4 KPI kartı: mod duyarlı metrikler (mevcut/genişletilecek)]
    ├── Map/
    │   ├── FireMap.tsx                  [İnteraktif harita, kümeleme, ay filtresi, lejant]
    │   └── HotspotPopover.tsx           [Desktop popover / mobil bottom-sheet nokta detayı]
    ├── Charts/
    │   ├── ComparisonPanel.tsx          [Raw vs Harmonised kaynak kırılımı ve delta özeti]
    │   └── SensorCliffChart.tsx         [Recharts zaman serisi + 2012 Sensor Cliff işaretçileri]
    └── Calendar/
        ├── BurningCalendar.tsx          [Year x Month ısı matrisi (CalendarCell[])]
        └── CalendarTooltip.tsx          [Hücre hover detayı: count, mean_frp, anomaly]
```

### Veri Akışı, Prop'lar ve State Sahipliği

| Bileşen / Hook | Tuttuğu State (Lokal) | Aldığı Prop'lar | Yaptığı / Tetiklediği İş |
| :--- | :--- | :--- | :--- |
| `App.tsx` | `mode: ViewMode`, `bbox`, `days`, `selectedMonth: { year: number; month: number } \| null`, `selectedHotspot: HarmonizedHotspot \| null`, `isUsingMock: boolean` | — | Tüm panelleri senkronize eden kök konteynerdir; `useHarmonizeData` verisini alt bileşenlere dağıtır. |
| `useHarmonizeData` | `data`, `loading`, `error` (`useApi` üzerinden) | `bbox`, `days` | `services/api.ts` içindeki `getHarmonized(...)` ve `getHealth()` çağrılarını yönetir; hata durumunda `SAMPLE_RESPONSE` geçişine izin verir. |
| `AppShell.tsx` | — | `children`, `healthStatus`, `onSelectScenario` | Üst başlıkta API sağlığını ve hızlı demo senaryo seçicisini gösterir. |
| `FilterBar.tsx` | `isMobileSheetOpen: boolean`, taslak bbox/days | `bbox`, `days`, `loading`, `onApply(bbox, days)` | Bölge veya gün aralığı değiştiğinde `App.tsx`'e yeni sorgu parametresi iletir. |
| `ViewToggle.tsx` | — | `mode: ViewMode`, `disabled?: boolean`, `onChange(mode)` | `"raw"` ↔ `"harmonized"` geçişini tetikler; yükleme sırasında kilitlenebilir. |
| `StatsBar.tsx` | — | `mode: ViewMode`, `raw`, `harmonizedCount`, `duplicatesRemoved`, `meanFrpDelta`, `calendarSummary` | Seçili `mode`'a göre 4 KPI kartının birincil sayılarını ve alt açıklamalarını değiştirir. |
| `FireMap.tsx` | `viewport` (zoom, center), `hoveredHotspot` | `mode: ViewMode`, `hotspots: HarmonizedHotspot[]`, `bbox`, `selectedMonth`, `selectedHotspot`, `onSelectHotspot` | `selectedMonth` varsa yalnızca o ayın noktalarını vurgular; tıklanan noktayı `HotspotPopover`'a iletir. |
| `HotspotPopover.tsx` | — | `hotspot: HarmonizedHotspot \| null`, `mode: ViewMode`, `onClose()` | `frp`, `frp_corrected`, `source`, `confidence`, `acquired`, `daynight` alanlarını gösterir. |
| `SensorCliffChart.tsx` | `activeSeriesHover` | `mode: ViewMode`, `calendar: CalendarCell[]`, `rawCountSummary` | Recharts üzerinde 2012 `ReferenceLine` ve `ReferenceArea` ile zaman serisini çizer. |
| `ComparisonPanel.tsx` | — | `mode: ViewMode`, `raw`, `harmonized` | `ModisOnly`, `ViirsOnly`, `Matched` dağılımını ve yüzde azalmayı özetler. |
| `BurningCalendar.tsx` | `hoveredCell: CalendarCell \| null`, `activeMetric: "anomaly" \| "count" \| "mean_frp"` | `calendar: CalendarCell[]`, `mode: ViewMode`, `selectedMonth`, `onSelectMonth(cell \| null)` | Yıl × Ay matrisini çizer; hücre tıklaması haritayı filtreler. |

---

## 9. Sonraki Görevlere Devir Notları

### C2 (Tasarım Sistemi: Renk, Tipografi, Spacing) İçin Devir Notları
- **C1'de Sabitlenenler:**
  - Semantik renk rollerinin varlığı sabitlendi: **Raw MODIS**, **Raw VIIRS**, **Harmonised Birleşik**, **4 Kademeli Anomali Skalası + Null Durumu**, **Uyarı / Çift Sayım Riski** ve **Yükleme/Hata Durumu**.
  - Bileşen hiyerarşisi (üst komuta şeridi, 4'lü `StatsBar`, 2 sütunlu desktop düzeni, mobil alt çekmece ve sekme yapısı) ve CSS custom properties kullanımı sabitlendi.
- **C2'ye Bırakılanlar:**
  - Tüm renk paleti, hex/HSL değerleri, CSS custom property isimlendirmeleri (`--color-*`), light/dark tema karşılıkları ve WCAG kontrast doğrulamaları.
  - Tipografi ailesi, boyut skalası, sayısal veriler için tabular/monospace kuralları, 8px spacing token'ları, köşe yuvarlaklıkları ve gölge katmanları.

### C3 (Harita + Takvim Detay Tasarımı) İçin Devir Notları
- **C1'de Sabitlenenler:**
  - Haritanın masaüstünde sol ana panelde (`~58%`), takvimin sağ altta yer alması.
  - Takvimin `Yıl (satır) × Ay (sütun)` matris yönelimi ve `anomaly: null` hücrelerinin veri yok/nötr gösterimi.
  - Takvim hücresine tıklamanın haritayı o aya filtrelemesi ve üst kenarda temizlenebilir filtre rozeti çıkarması.
  - Hotspot detayının masaüstünde `HotspotPopover`, mobilde alttan açılan Bottom Sheet olması; loading/empty/error durumlarının yerleşimi.
- **C3'e Bırakılanlar:**
  - Harita motoru/altlık harita seçimi ve zoom davranışları, küme (cluster) kırılım eşiği.
  - Nokta yarıçapı ve opaklığının `frp` / `frp_corrected` ve `confidence` (`Low | Nominal | High`) değerlerine tam eşleme kuralları.
  - Takvim hücre boyutları, kaydırma (çok yıllı veri için dikey scroll) ve `CalendarTooltip` iç düzeni.

### C4 (Raw ↔ Harmonized Karşılaştırma Tasarımı) İçin Devir Notları
- **C1'de Sabitlenenler:**
  - Geçiş modeli olarak yan yana veya swipe yerine **yerinde (in-place) cross-fade + senkronize durum güncellemesi** seçildi.
  - Toggle ile eşzamanlı değişen 3 katman (harita noktaları, `StatsBar` metrikleri, `SensorCliffChart` eğrisi) ve Harmonised modda Raw eğrisinin soluk hayalet referans olarak kalması sabitlendi.
  - 2012 Sensor Cliff işaretleme anatomisi (`ReferenceLine` + tepe etiketi + `ReferenceArea` gölgeli dönem ayrımı) belirlendi.
- **C4'e Bırakılanlar:**
  - Geçiş animasyonlarının milisaniye süreleri, easing eğrileri ve harita noktalarının birleşme/kaybolma mikro-animasyonları.
  - `HarmonizeResponse` şeması henüz ham nokta koordinat dizisi dönmediğinden, Raw moddaki harita noktalarının `HarmonizedHotspot.source` ve `frp` üzerinden türetilme kuralı (veya backend'den ek alan istenip istenmeyeceği kararının netleştirilmesi).
