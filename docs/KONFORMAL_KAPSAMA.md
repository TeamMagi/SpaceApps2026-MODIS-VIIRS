# Konformal Kapsama Raporu (S10)

Bu belge `data/conformal.json` tablosunun nasıl kalibre edildiğini ve ölçülen kapsamayı verir. Tablo `calibration_fingerprint = fnv1a64:d44d0890689bef01` taşır. Üretici: `crates/firms-client/examples/conformal_calibration.rs` (`extract` ve `fit`).

## 1. Bu bant neyi ölçer

Gerçek yayılan enerji (FRP) için bir referans doğru yoktur. Bantlar bir **sensör-dışarıda (held-out-sensor) tekrarlanabilirlik** ölçüsüdür; ölçüm doğruluğu veya doğrulanmış ground-truth kapsaması değildir.

- **Dönem A (`pre_viirs`):** μ = yalnız MODIS girdisiyle yeniden koşulan pipeline'ın (Terra+Aqua, `k_c` düzeltmeli) ülke/ay `total_frp`'si; y = aynı ay için tam MODIS+VIIRS pipeline `total_frp`'si. Aylar 2012-02..2025-12.
- **Dönem B (`viirs`):** μ ve y, SNPP-only ve NOAA-20-only `total_frp`'dir; yön (ülke, ay) hash paritesiyle sabittir. Aylar 2018-05..2025-12.
- Kalıntı `e = (y-μ)/μ`; bölme = (dönem, μ büyüklük kutusu); aralık = eşit-kuyruklu ampirik kantil (%68, %90). Kenar ve kantiller yalnız kalibrasyon katında hesaplanır.

## 2. Kullanılan ülkeler

61 ülke: Albania, Algeria, Argentina, Armenia, Azerbaijan, Bolivia, Bulgaria, Cambodia, Canada, Chile, Colombia, Croatia, Cuba, Cyprus, Ecuador, Finland, France, Georgia, Germany, Ghana, Greece, Guatemala, Honduras, Hungary, Ireland, Israel, Italy, Kazakhstan, Kenya, Lao_PDR, Lebanon, Madagascar, Mexico, Mongolia, Morocco, Myanmar, Nepal, New_Zealand, Nicaragua, Norway, Panama, Paraguay, Peru, Poland, Portugal, Romania, Serbia, Slovakia, South_Africa, Spain, Sri_Lanka, Sweden, Tanzania, Thailand, Tunisia, Turkey, Uganda, Ukraine, United_Kingdom, Uruguay, Vietnam.

**Eksik G12 (ağır) ülkeler:** Brazil, Angola, Australia, Indonesia, India, United_States, Russian_Federation. Koşu zaman bütçesiyle kesildi; en büyük μ kutuları bu yüzden zayıf temsil edilmiş olabilir.

Planlanıp tamamlanmayan ülkeler: Brazil, Angola, Australia, Indonesia, India, United_States, Russian_Federation, Zambia, Mozambique, Nigeria.

## 3. Tablo: örnek sayıları ve kutular

### Dönem `pre_viirs` (n = 9515)

| Kutu | μ alt (MW) | μ üst (MW) | n_cal | q68 alt | q68 üst | q90 alt | q90 üst |
|---|---|---|---|---|---|---|---|
| 0 | - | 173.490 | 1585 | 0.535 | 5.778 | 0.072 | 11.741 |
| 1 | 173.490 | 746.656 | 1586 | 0.311 | 2.722 | 0.008 | 4.623 |
| 2 | 746.656 | 2198.601 | 1586 | 0.284 | 2.144 | 0.030 | 3.678 |
| 3 | 2198.601 | 8969.151 | 1586 | 0.263 | 1.738 | 0.018 | 3.081 |
| 4 | 8969.151 | 50412.033 | 1586 | 0.210 | 1.583 | -0.003 | 2.486 |
| 5 | 50412.033 | - | 1586 | 0.122 | 1.146 | -0.009 | 1.690 |

### Dönem `viirs` (n = 5581)

| Kutu | μ alt (MW) | μ üst (MW) | n_cal | q68 alt | q68 üst | q90 alt | q90 üst |
|---|---|---|---|---|---|---|---|
| 0 | - | 115.285 | 930 | -0.470 | 1.078 | -0.779 | 2.959 |
| 1 | 115.285 | 605.530 | 930 | -0.343 | 0.471 | -0.527 | 1.159 |
| 2 | 605.530 | 1874.015 | 930 | -0.253 | 0.331 | -0.459 | 0.601 |
| 3 | 1874.015 | 7751.612 | 930 | -0.211 | 0.180 | -0.356 | 0.442 |
| 4 | 7751.612 | 41881.136 | 930 | -0.127 | 0.116 | -0.262 | 0.240 |
| 5 | 41881.136 | - | 931 | -0.074 | 0.071 | -0.157 | 0.147 |

## 4. Ölçülen kapsama

### Dönem `pre_viirs`

Medyan y/μ = 1.838; medyan işaretli kalıntı = 83.8%.

**LOYO (yıl bloğu dışarıda):**

| Kutu | n | %68 kapsama | %90 kapsama | Analitik bant |
|---|---|---|---|---|
| 0 | 1586 | 67.5% | 89.9% | 8.1% |
| 1 | 1589 | 67.8% | 90.1% | 9.4% |
| 2 | 1581 | 67.4% | 89.8% | 9.0% |
| 3 | 1589 | 67.6% | 89.6% | 9.0% |
| 4 | 1585 | 67.4% | 90.1% | 10.7% |
| 5 | 1585 | 67.4% | 89.5% | 16.7% |
| **Havuz** | 9515 | 67.5% | 89.8% | 10.5% |

**LOCO (ülke bloğu dışarıda):**

| Kutu | n | %68 kapsama | %90 kapsama | Analitik bant |
|---|---|---|---|---|
| 0 | 1606 | 67.8% | 89.7% | 8.2% |
| 1 | 1574 | 67.7% | 89.6% | 9.6% |
| 2 | 1580 | 67.8% | 89.9% | 8.8% |
| 3 | 1566 | 67.4% | 89.6% | 9.1% |
| 4 | 1566 | 67.3% | 89.3% | 10.6% |
| 5 | 1623 | 66.7% | 89.0% | 16.5% |
| **Havuz** | 9515 | 67.5% | 89.5% | 10.5% |

**Yıl-bloklu bootstrap** (B=1000, seed=42, 14 yıl bloğu), %90 GA: %68 bandı [66.4%, 68.4%], %90 bandı [89.0%, 90.6%].

### Dönem `viirs`

Medyan y/μ = 0.995; medyan işaretli kalıntı = -0.5%.

**LOYO (yıl bloğu dışarıda):**

| Kutu | n | %68 kapsama | %90 kapsama | Analitik bant |
|---|---|---|---|---|
| 0 | 931 | 67.8% | 90.5% | 4.0% |
| 1 | 933 | 67.7% | 89.9% | 3.8% |
| 2 | 927 | 68.5% | 90.1% | 2.7% |
| 3 | 930 | 67.5% | 90.0% | 2.4% |
| 4 | 932 | 68.7% | 90.1% | 2.3% |
| 5 | 928 | 67.8% | 89.9% | 2.9% |
| **Havuz** | 5581 | 68.0% | 90.1% | 3.0% |

**LOCO (ülke bloğu dışarıda):**

| Kutu | n | %68 kapsama | %90 kapsama | Analitik bant |
|---|---|---|---|---|
| 0 | 954 | 67.8% | 90.1% | 4.0% |
| 1 | 919 | 68.3% | 90.4% | 3.7% |
| 2 | 918 | 68.4% | 90.1% | 2.7% |
| 3 | 926 | 68.1% | 90.4% | 2.4% |
| 4 | 908 | 68.1% | 90.2% | 2.3% |
| 5 | 956 | 68.4% | 89.7% | 2.8% |
| **Havuz** | 5581 | 68.2% | 90.2% | 3.0% |

**Yıl-bloklu bootstrap** (B=1000, seed=42, 8 yıl bloğu), %90 GA: %68 bandı [65.4%, 70.4%], %90 bandı [88.2%, 91.7%].

## 5. Kabul ölçütleri

Ölçütler tasarımdan (§5) alınmıştır; seviyeler kapsama sonucuna bakılarak ayarlanmamıştır.

### Dönem `pre_viirs`: **FAIL**

- PASS: LOYO havuz %68 kapsaması 67.5% in [64,0%; 76,0%]
- PASS: LOYO havuz %90 kapsaması 89.8% in [87,0%; 96,0%]
- PASS: LOCO havuz %68 kapsaması 67.5% in [64,0%; 76,0%]
- PASS: LOCO havuz %90 kapsaması 89.5% in [87,0%; 96,0%]
- PASS: bootstrap %90 bandı GA alt ucu 89.0% >= 85,0%
- PASS: bootstrap %68 bandı GA alt ucu 66.4% >= 63,0%
- PASS: n>=100 her kutuda kapsama >= nominal-0,08 (LOYO)
- FAIL: %90 bandın medyan göreli genişliği 3.14 < 3

### Dönem `viirs`: **PASS**

- PASS: LOYO havuz %68 kapsaması 68.0% in [64,0%; 76,0%]
- PASS: LOYO havuz %90 kapsaması 90.1% in [87,0%; 96,0%]
- PASS: LOCO havuz %68 kapsaması 68.2% in [64,0%; 76,0%]
- PASS: LOCO havuz %90 kapsaması 90.2% in [87,0%; 96,0%]
- PASS: bootstrap %90 bandı GA alt ucu 88.2% >= 85,0%
- PASS: bootstrap %68 bandı GA alt ucu 65.4% >= 63,0%
- PASS: n>=100 her kutuda kapsama >= nominal-0,08 (LOYO)
- PASS: %90 bandın medyan göreli genişliği 1.02 < 3

## 6. Analitik bant ile karşılaştırma

Aynı hücrelerde, aynı `y` için μ hücresinin analitik `total_frp_lower/upper` bandının kapsaması (yan rapor, geçti/kaldı değildir):

| Dönem | Analitik bant | Conformal %68 | Conformal %90 |
|---|---|---|---|
| `pre_viirs` | 10.5% | 67.5% | 89.8% |
| `viirs` | 3.0% | 68.0% | 90.1% |

Analitik bandın nominal seviyesi yaklaşık 1σ'dır; kıyas yön göstericidir. Analitik bant zaten nominalin üstünde çıkarsa S10'un öncülü bizim veride çürümüş demektir.

## 7. Dürüst sınırlamalar

- **Doğruluk değil tekrarlanabilirlik:** bant gerçek FRP'ye karşı doğrulanmamıştır. "Doğrulanmış kapsama" denemez; doğrusu "sensör-dışarıda tekrarlanabilirlik bandı, yıl-dışarıda kapsama"dır.
- **Dönem A yanlılığı:** dönem A'da medyan y/μ = 1.84 (Türkiye ön ölçümünde ortalama 1,78). Yalnız MODIS + `k_c` ile kurulan toplam, VIIRS'li tam pipeline'a göre sistematik olarak düşük kalır (VIIRS ek tespit ekler); bu yüzden bant asimetriktir ve üst yöne kaymıştır. İşaretli kantil bunu yakalar ama bant geniştir.
- **Aktarım varsayımdır:** 2002-07..2011 (dönem A) ve 2012..2017 (tek VIIRS, dönem B) için kapsama ölçülmedi; ölçüm 2012-2025 ve 2018-2025'te yapıldı. Yörünge kayması ve farklı yangın rejimi durağanlığı sınanamaz.
- **Dönem B tek-filo çiftidir:** kaynaşık (MODIS+2×VIIRS) hücrenin gerçek hatasını ölçmez; tek-filo kopyası kabaca √2 geniş olduğundan muhafazakârdır.
- **Marjinal garanti:** değiştirilebilirlik varsayımı yıl şokları ve komşu ülke korelasyonuyla bozulur; LOYO ve LOCO bunu ölçmek içindir.
- **`k_c` kısmen in-sample:** 2019 fit / 2020 hold-out nedeniyle bu yıllar dönem A için tam bağımsız değildir.
- **Ülke örneklemi:** küçük ve orta ülkeler küçük-μ kutularını, G12 ülkeleri büyük kutuları besler; eksik ağır ülkeler üst kutuyu zayıflatır.
- **Kapsam:** yalnız ülke toplamı `total_frp` içindir; bbox veya `min_confidence` süzgeçli sorgulara uygulanmaz.
- **Merkez değişimi:** R10/K11 gibi `total_frp` merkezini değiştiren değişiklikler `calibration_fingerprint`'i bozar; tablo yeniden üretilmelidir.

