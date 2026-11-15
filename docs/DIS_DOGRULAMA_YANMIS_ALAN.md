# Dış Doğrulama Pilotu — ESA Fire_cci Yanmış Alanına Karşı Ham vs Harmonize Seri (Y10)

> **Görev:** Y10 (pilot) · **Dönem:** 2012–2020 (VIIRS dönemi, 108 ay) · **Referans:** ESA Fire_cci v5.1 (FireCCI51) 0.25° aylık grid `burned_area`
> **Üretici:** `crates/firms-client/examples/burned_area_validation.rs` + `tools/firecci_extract.py` · **Çıktılar:** `docs/vaka/firecci_validation_monthly.csv`, `docs/vaka/firecci_validation_summary.json`
>
> Tüm sayılar **(Ölçüm)**: kendi FIRMS arşivimiz (`.cache/firms-archive`, SP, `type 0`) ve indirilen Fire_cci dosyalarından bu programla hesaplanmıştır. `fire-harmony` CCC `0.87 → 0.94` değeri yalnız karşılaştırma referansıdır.

## 1. Soru

Harmonize seri (`/api/calendar` `count_normalized` ve `total_frp`), bağımsız bir yanmış alan ürünüyle ham FIRMS serisinden daha uyumlu mu? Uyum Lin's CCC ile, mekânsal tutarlılık hücre-ay omisyon/komisyon (IOE/ICE) ile ölçüldü.

## 2. Yöntem

### 2.1 Veri
- **Fire_cci:** CEDA açık HTTP sunucusu, giriş gerektirmez: `https://dap.ceda.ac.uk/neodc/esacci/fire/data/burned_area/MODIS/grid/v5.1/Compressed/<yıl>.zip` (yıl başına ~17–22 MB, 2012–2020 toplam ~190 MB, `.cache/firecci/zip`, gitignore'da). Dosyalar NetCDF-3 classic; `tools/firecci_extract.py` (Python 3 + numpy + scipy) `burned_area > 0` hücre-aylarını tek CSV'ye düzleştirir. 2019 için düzeltilmiş `2019.zip` kullanıldı (Ekim–Aralık 2019 errata'sı).
- **FIRMS:** `modis/`, `viirs-snpp/`, `viirs-jpss1/` ülke-yıl dosyaları, yalnız `type 0`.
- **Pipeline:** `PipelineConfig::default()`; her takvim yılı tek parti `harmonize_with_links_config`; takvim (`build_calendar_with`) iki dönemde kurulur (`2012-01..2018-03` MODIS+S-NPP, `2018-04..2020-12` +NOAA-20), Ω dönem başına havuzlanır (`china_ban_case` ile aynı yaklaşım).

### 2.2 Bölgeler
Ülke dosyası ile 0.25° grid arasında sınır uyuşmazlığı olmasın diye (komşu ülkenin yanan hücresi, bizim dosyamızda hotspot olmadığı için yapay omisyon üretirdi) her bölge **tamamen ülke içinde kalan, 0.25° gride oturan kutulardan** oluşur. Hücre, merkezi kutudaysa bölgeye dahildir; hotspotlar da hücrelerine göre süzülür.

| Bölge | Ülke dosyası | Kutular `[B, G, D, K]` | Hücre | Yanmış hücre-ay | Fire_cci toplam (km², 9 yıl) | Ω (dönem 1 / 2) |
|---|---|---|---|---|---|---|
| Türkiye (Anadolu iç kutusu) | Turkey | `[27.25, 37.25, 43.0, 41.0]` | 945 | 3,417 | 28,100 | 4.03 / 6.55 |
| Brezilya (Cerrado / ormansızlaşma yayı) | Brazil | `[-58, -15, -44, -5]` | 2,240 | 52,038 | 951,259 | 4.01 / 5.81 |
| Kanada (AB/SK/MB boreal) | Canada | `[-120, 50, -95, 60]` | 4,000 | 5,982 | 72,546 | 4.79 / 5.94 |
| Hindistan (orta + Pencap/Haryana) | India | `[74, 16, 84, 27]` ∪ `[74.75, 27, 79.5, 30.75]` | 2,045 | 16,381 | 361,705 | 7.47 / 11.26 |

Rusya yerine Kanada seçildi (boreal temsilci).

### 2.3 Seriler
- `raw_modis` (Terra+Aqua), `raw_snpp`, `raw_all` (MODIS + S-NPP + NOAA-20 ham toplam — uygulamadaki `raw_total`)
- `harmonized`: bölge-ayda takvimin kendi `count_normalized` ve `total_frp` alanları; hücre-ayda harmonize hotspot sayısı `/ Ω(ay)` ve `Σ frp_corrected`.

### 2.4 Metrikler
- **Dönüşüm:** her seri `ln(1 + x / ortalama(x))` ile birimsizleştirilir (km² vs tespit vs MW). Z-skor kullanılmadı: iki seriyi z-skorlamak ortalama ve varyansı eşitler ve CCC'yi Pearson r'ye indirger; bu dönüşüm ise ölçek/dağılım farkını CCC'de cezalandırmaya devam eder.
- **Lin's CCC** `= 2·cov / (var_x + var_y + (μ_x − μ_y)²)` (popülasyon momentleri) ve aynı dönüşümde Pearson r.
- **Bölge-ay (RM, n = 108)** ve **hücre-ay (CM, n = hücre × 108, sıfırlar dahil)**.
- **Mevsimsellikten arındırılmış RM CCC ("anom"):** dönüştürülmüş seriden her takvim ayının 9 yıllık ortalaması çıkarılır. Ortak mevsim döngüsü ham CCC'yi şişirdiği için yıllar arası uyumu (ve 2018 NOAA-20 seviye sıçramasını) bu metrik gösterir.
- **Omisyon/komisyon (hücre-ay):** yanmış = `burned_area > 0`, hotspot = hücre-ayda ≥ 1 tespit. `IOE = yanmış ∧ hotspot yok / yanmış`, `ICE = hotspot ∧ yanmış değil / hotspot`, `Dice = 2·ortak / (2·ortak + IOE payı + ICE payı)`.

## 3. Sonuçlar (Ölçüm)

### 3.1 Özet — `raw_all` → `harmonized`

| Bölge | CCC sayım (RM) | CCC FRP (RM) | CCC sayım anom (RM) | CCC FRP anom (RM) | CCC sayım (CM) | CCC FRP (CM) | IOE | ICE |
|---|---|---|---|---|---|---|---|---|
| Türkiye | 0.839 → **0.873** | 0.910 → 0.908 | 0.572 → **0.755** | 0.722 → 0.738 | 0.552 → 0.530 | 0.633 → 0.602 | 0.054 → 0.056 | 0.892 → 0.892 |
| Brezilya | 0.952 → **0.965** | 0.976 → 0.975 | 0.658 → **0.878** | 0.809 → **0.862** | 0.812 → 0.797 | 0.829 → 0.817 | 0.017 → 0.018 | 0.635 → 0.634 |
| Kanada | 0.916 → **0.935** | 0.931 → 0.937 | 0.875 → **0.919** | 0.888 → 0.903 | 0.651 → 0.628 | 0.743 → 0.721 | 0.190 → 0.194 | 0.868 → 0.868 |
| Hindistan | 0.890 → **0.921** | 0.891 → 0.898 | 0.637 → **0.791** | 0.675 → 0.713 | 0.660 → 0.653 | 0.678 → 0.661 | 0.009 → 0.010 | 0.843 → 0.843 |

### 3.2 Tüm seriler

Sütunlar: CCC sayım / FRP (RM), Pearson r sayım / FRP (RM), CCC anom sayım / FRP (RM), CCC sayım / FRP (CM), IOE, ICE, Dice.

**Türkiye**

| Seri | CCC c | CCC f | r c | r f | anom c | anom f | CM c | CM f | IOE | ICE | Dice |
|---|---|---|---|---|---|---|---|---|---|---|---|
| raw_modis | 0.857 | 0.936 | 0.885 | 0.942 | 0.783 | 0.826 | 0.567 | 0.656 | 0.151 | 0.756 | 0.379 |
| raw_snpp | 0.890 | 0.921 | 0.903 | 0.927 | 0.760 | 0.794 | 0.551 | 0.602 | 0.086 | 0.876 | 0.219 |
| raw_all | 0.839 | 0.910 | 0.854 | 0.918 | 0.572 | 0.722 | 0.552 | 0.633 | 0.054 | 0.892 | 0.194 |
| harmonized | 0.873 | 0.908 | 0.895 | 0.921 | 0.755 | 0.738 | 0.530 | 0.602 | 0.056 | 0.892 | 0.194 |

**Brezilya**

| Seri | CCC c | CCC f | r c | r f | anom c | anom f | CM c | CM f | IOE | ICE | Dice |
|---|---|---|---|---|---|---|---|---|---|---|---|
| raw_modis | 0.965 | 0.989 | 0.988 | 0.993 | 0.906 | 0.943 | 0.801 | 0.824 | 0.059 | 0.519 | 0.637 |
| raw_snpp | 0.978 | 0.984 | 0.984 | 0.986 | 0.914 | 0.916 | 0.819 | 0.819 | 0.050 | 0.589 | 0.574 |
| raw_all | 0.952 | 0.976 | 0.960 | 0.978 | 0.658 | 0.809 | 0.812 | 0.829 | 0.017 | 0.635 | 0.533 |
| harmonized | 0.965 | 0.975 | 0.978 | 0.981 | 0.878 | 0.862 | 0.797 | 0.817 | 0.018 | 0.634 | 0.533 |

**Kanada**

| Seri | CCC c | CCC f | r c | r f | anom c | anom f | CM c | CM f | IOE | ICE | Dice |
|---|---|---|---|---|---|---|---|---|---|---|---|
| raw_modis | 0.912 | 0.930 | 0.927 | 0.931 | 0.900 | 0.890 | 0.632 | 0.744 | 0.251 | 0.739 | 0.387 |
| raw_snpp | 0.940 | 0.944 | 0.946 | 0.945 | 0.918 | 0.912 | 0.659 | 0.728 | 0.224 | 0.854 | 0.245 |
| raw_all | 0.916 | 0.931 | 0.924 | 0.931 | 0.875 | 0.888 | 0.651 | 0.743 | 0.190 | 0.868 | 0.227 |
| harmonized | 0.935 | 0.937 | 0.945 | 0.938 | 0.919 | 0.903 | 0.628 | 0.721 | 0.194 | 0.868 | 0.227 |

**Hindistan**

| Seri | CCC c | CCC f | r c | r f | anom c | anom f | CM c | CM f | IOE | ICE | Dice |
|---|---|---|---|---|---|---|---|---|---|---|---|
| raw_modis | 0.861 | 0.909 | 0.888 | 0.919 | 0.779 | 0.783 | 0.663 | 0.681 | 0.104 | 0.670 | 0.483 |
| raw_snpp | 0.933 | 0.921 | 0.950 | 0.931 | 0.807 | 0.790 | 0.666 | 0.674 | 0.019 | 0.832 | 0.287 |
| raw_all | 0.890 | 0.891 | 0.905 | 0.899 | 0.637 | 0.675 | 0.660 | 0.678 | 0.009 | 0.843 | 0.270 |
| harmonized | 0.921 | 0.898 | 0.943 | 0.910 | 0.791 | 0.713 | 0.653 | 0.661 | 0.010 | 0.843 | 0.271 |

## 4. Yorum

1. **Bölge-ay sayım serisinde harmonizasyon her 4 bölgede CCC'yi artırır** (`raw_all → harmonized`: Türkiye `0.839 → 0.873`, Brezilya `0.952 → 0.965`, Kanada `0.916 → 0.935`, Hindistan `0.890 → 0.921`). Etki asıl **mevsimsellikten arındırılmış** seride görünür: `0.572 → 0.755`, `0.658 → 0.878`, `0.875 → 0.919`, `0.637 → 0.791`. Ham toplam serideki 2018 NOAA-20 seviye sıçraması yanmış alanda yoktur; Ω normalizasyonu bunu giderir. `fire-harmony` referansı (`0.87 → 0.94`) ile aynı yönde ve benzer büyüklükte; ancak metrik tanımı, bölge ve dönem aynı değildir, sayılar doğrudan karşılaştırılamaz.
2. **FRP serisinde kazanç küçük veya yok** (RM `±0.01`; anom'da Brezilya `0.809 → 0.862`, diğerleri `+0.02–0.04`). Harmonize `total_frp` VIIRS döneminde sensör birleştirmeden geçen FRP toplamıdır; NOAA-20 eklenmesi FRP'yi sayım kadar şişirmez.
3. **Harmonize seri tek sensörlü ham seriyi geçmez.** `raw_modis` / `raw_snpp` zaten filo değişmeyen tutarlı akışlardır ve Fire_cci MODIS türevi olduğu için `raw_modis` FRP en yüksek RM CCC'yi verir (Brezilya `0.989`, Türkiye `0.936`). Harmonizasyonun iddiası tek sensörü yenmek değil, **çok sensörlü toplamın** sürekliliğini sağlamaktır; bu ölçüm o iddiayı destekler.
4. **Hücre-ay CCC ve IOE/ICE'de harmonizasyon fark yaratmaz (beklenen).** Harmonizasyon tespitleri birleştirir/maskeler, yeni konum üretmez; bu yüzden harmonize hotspot varlığı ham `raw_all` varlığının alt kümesidir: ICE değişmez, IOE `≤ 0.004` artar, hücre-ay CCC `0.01–0.03` düşer (Ω ile ölçek küçülünce sıfır olmayan küçük değerler sıfıra yaklaşır). Statik maske bu dört kutuda hücre-ay varlığını pratikte değiştirmedi.
5. **Komisyon yüksek (ICE `0.63–0.89`).** 375 m VIIRS ve 1 km MODIS, Fire_cci'nin (250 m MODIS yansıtımı) yakalayamadığı küçük tarımsal ve kısa süreli yanmaları görür; Türkiye ve Hindistan'daki anız yakmaları bunun tipik örneğidir. Yalnız MODIS ICE daha düşüktür (`0.52–0.76`) ama omisyonu yüksektir (Kanada `0.251`). Omisyon VIIRS eklendikçe düşer (Hindistan `0.104 → 0.009`). Bu sayılar "hata" değil, iki ürünün farklı şeyi ölçtüğünü gösterir.

## 5. Sınırlar (dürüst)

- **Fire_cci tamamen bağımsız değildir:** FireCCI51, MODIS MOD09GQ yansıtımı + **MOD14ML aktif yangınları** (tohum noktası) ile üretilir. MODIS hotspotlarıyla uyumu kısmen yapısaldır; `raw_modis`'in yüksek skorları buna bağlıdır. VIIRS ve harmonize seriler için daha bağımsızdır ama tam değil.
- **Sentinel-2 20 m dNBR ve 30 m Landsat referansları bu pilotta kullanılmadı** (kapsam dışı; görev tanımında anılan `[25]`, `[01, 10]` karşılaştırmaları yapılmadı). FireCCIS311 (OLCI) ve MCD64A1 alternatiflerine gerek kalmadı (Fire_cci girişsiz erişilebildi); denenmediler.
- **Dönem 2012–2020:** Fire_cci 5.1 2022'ye kadar var ama yalnız 2012–2020 indirildi; MODIS-only dönemi (2000–2011) ve NOAA-21 kapsanmadı.
- **Bölgeler ülke içi kutulardır**, ülkenin tamamı değildir (sınır hücreleri bilinçli dışlandı; kıyı/deniz hücreleri her iki üründe de boş olduğu için IOE/ICE'yi etkilemez). Kutu dışı biyomlar (ör. Amazon ormanı çekirdeği, Rusya boreal) ölçülmedi.
- **Varlık eşiği en duyarlı tanımdır** (`BA > 0`, `≥ 1` tespit). Alan eşiği, `fraction_of_observed_area` ile bulut maskesi ve zaman penceresi kaydırması (ay sınırını aşan yanma) denenmedi; IOE/ICE bu seçimlere duyarlıdır.
- **Tek dönüşüm:** CCC değerleri `ln(1 + x/ortalama)` seçimine bağlıdır; farklı ölçekleme (ör. doğrusal kalibrasyon sonrası) farklı mutlak değer verir. Raw vs harmonize sıralaması aynı dönüşümle karşılaştırıldığı için anlamlıdır.
- Güven aralığı (blok bootstrap) hesaplanmadı; RM'de n = 108, anom CCC farkları 0.04'ün altındaysa (Kanada FRP, Türkiye FRP) anlamlılık iddia edilmez.
- Takvim dönemlerinde Türkiye `2018-04..2020-12` için program "2 platform yapılandırması" uyarısı verdi: düşük yangınlı birkaç ayda harmonize satırlarında NOAA-20 hiç yok. Ω tek `/api/calendar` çağrısıyla bu nedenle küçük farklılık gösterebilir; etkisi ihmal edilebilir düzeydedir.

## 6. Yeniden üretme

```bash
mkdir -p .cache/firecci/zip && cd .cache/firecci/zip
for y in 2012 2013 2014 2015 2016 2017 2018 2019 2020; do
  curl -sS -O "https://dap.ceda.ac.uk/neodc/esacci/fire/data/burned_area/MODIS/grid/v5.1/Compressed/$y.zip"
done
cd ../../..
python -I tools/firecci_extract.py .cache/firecci/zip .cache/firecci/firecci51_nonzero.csv   # ~45 sn
cargo run --release -p pyrosync-firms --example burned_area_validation                      # ~2,5 dk
```

Kaynak: Chuvieco, E. vd. (2018), ESA Fire_cci MODIS Burned Area Grid product v5.1, CEDA, doi:10.5285/3628cb2fdba443588155e15dee8e5352.
