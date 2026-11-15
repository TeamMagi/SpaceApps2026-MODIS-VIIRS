# Yusuf geliştirme fixture'ları (Y5)

Bu paket sentetiktir. NASA verisi, saha gerçeği veya Rust harmonizasyon çıktısı değildir.
API anahtarı ve tarihsel arşiv indirmesi gerektirmez.

## İçerik

- `metadata.json`: seed `20261115`, sürüm, sabit üretim zamanı, kapsam açıklaması.
- `hotspots.json`: 2021 Türkiye/Akdeniz sentetik kümesinin 800 ham, 500 harmonize kaydı.
  100 interscan elemesi + 100 filo birleştirmesi + 100 sensör eşleştirmesi.
  Koordinatlar 29–31 E, 36.5–37.1 N; bütün Türkiye'yi temsil etmez.
- `raw-platforms.json`: ham nokta kimliğinden platforma eşleme.
- `calendar.json`: `SeriesResponse` biçiminde 2000–2025 özeti. 312 UI ay slotunun
  ilk 10'u kapsama dışıdır; dosyada 302 gözlem/özet ayı vardır. 2021 özetleri detay
  noktalarından, diğer yıllar seed'li sentetik sayılardan üretilir. Bu dosya core
  `CalendarCell[]` değildir. Ek `frpMonths` anahtarı (`FrpMonthlyPoint`, Y1 için önerilen
  model; mevcut API alanı değil) her ay için sensör bazlı sayı/FRP toplamı ile
  `meanFrpRaw` (toplam/toplam) ve `meanFrpHarmonized` verir; payda 0 ise `null`.
  `index.ts` bunu `mockSeries`'ten ayırıp `mockFrpMonthly` olarak export eder.
  2021 toplamları noktalardan; diğer yıllar sayı × 2021 sensör ortalaması × (1 ± %10 seed'li gürültü).
- `stats.json`: aynı 2021 noktalarından yıllık ve aylık kart girdileri.
- `index.ts`: camelCase UI tipleriyle export; enum dönüşümleri açıkça yapılır.

Özet sayım harmonize olsa da Ω-normalize değildir. 2012 basamağının kaybolduğu
iddia edilmez. Sentetik FRP düzeltmesi yalnız görünüm geliştirmek içindir.
VIIRS (375 m) noktaları MODIS dağılımının ~0.35 katı FRP üretir; harmonize `frpCorrected`
VIIRS tabanlı kayıtlarda `frp / SYNTHETIC_VIIRS_TO_MODIS_EQ` (0.35) ile hesaplanır. Bu,
2012 basamağını görsel olarak göstermek için kullanılan görsel-amaçlı bir köprüdür;
pipeline katsayısı değildir. `schemaVersion` 2'dir.
Üretim zamanı sabittir; son NASA güncellemesi anlamına gelmez.

## Yeniden üretme

Repo kökünden `node web/scripts/generate-mocks.mjs` çalıştırılır. Bu komut yukarıdaki
altı JSON dosyasını aynı seed ile yeniden yazar. JSON dosyaları elle düzenlenmez;
senaryo değişikliği üreticide yapılır. Node dışında bağımlılığı yoktur.

## Bileşen önizlemesi

Frontend bağımlılıkları hazır olduğunda `web/` içinde `npm run dev` çalıştırılıp
`http://localhost:5173/mock.html` açılır (Vite başka port seçerse o port kullanılır).
Bu ayrı geliştirme girişidir; varsayılan production build'in girişine eklenmedi.
Ana uygulama mock'a yönlendirilmez ve otomatik API fallback'i kurulmaz.

Önizlemede ay/bütün yıl seçimi, Raw/Harmonized, EN/TR ve tema kontrolleri bulunur.
Bar grafiği daima 2021 aylık ham sayım referansıdır; donut ve kartlar seçilen dönemi
gösterir. Sayısal harmonize sonuçlar ortak geçişin `revealAnswer` aşamasını bekler.
Grafikler ve kartlar props alır; mock bağımlılığı önizleme girişindedir.

## Doğrulama durumu

Çalıştırıldı ve geçti (`web/`, 2026-11-15):

- Üretici iki kez çalıştırıldı; `sha256sum` çıktıları birebir aynı (deterministik).
- `npm run typecheck`: hatasız.
- `npm test`: `tests/mocks.test.ts` dahil tümü geçti (sayım/aralık, bağlantı bütünlüğü,
  aşama toplamları, seri kapsamı, mevsimsellik, 2012 uçurumu, frpMonths tutarlılığı, 2021 eşleşmesi).
  Ortalama alanları 3 ondalığa yuvarlandığı için test toleransı 1e-3'tür.
- `npm run build`: başarılı.
- 2011 -> 2012: ham sayı 277 -> 894; ham ortalama FRP 128.6 -> 74.3 MW (-%42);
  harmonize ortalama FRP 136.0 -> 141.4 MW (+%3.9).

Tarayıcıda görsel kontrol yapılmadı. Y2 scatter, gerçek veri bağlantısı, Y1 ve Y3 bu
teslimin dışında. Y4'te karşılaştırılabilir geçmiş detay yokken trend oku üretilmez.
