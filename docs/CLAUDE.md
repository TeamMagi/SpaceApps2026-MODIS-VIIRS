# docs — Proje Dokümantasyonu

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- Teknik dokümantasyon Türkçe yazılır (MIMARI.md, API referans vb.).
- README.md İngilizce kalır (jüri değerlendirmesi için).

## Mevcut Dokümanlar

| Dosya | İçerik |
|-------|--------|
| `MIMARI.md` | Proje mimarisi, dizin yapısı, pipeline açıklaması, tech stack |
| `API.md` | Sunucu API referansı: 11 endpoint, parametreler, örnek istek/yanıt, hata kodları, ortam değişkenleri, bilinen sorunlar |
| `UI-UX-KONSEPT.md` | UI/UX konsepti, wireframe'ler, kullanıcı akışı, bileşen ağacı (C1) |
| `TASARIM_KONSEPT.md` | UI/UX tasarım konsepti, geçiş koreografisi, yerleşim ve ekran listesi |
| `UX_YAZIM.md` | Ses tonu, mikro metin kuralları, EN/TR metin tablosu ve biçim standartları |
| `FRONTEND_STACK_ONERISI.md` | Frontend teknoloji yığını kararları, uygulanan prototip seçimi ve API sözleşmesi |
| `REFERANS_ANALIZI.md` | 32 makale + 6 referans projenin sentezi: açık sorunlara (P1–P7) göre bulgular, önceliklendirilmiş yol haritası, makale envanteri |
| `TASARIM_METHODS.md` | Methods sayfası/çekmecesi, 7 aşamalı pipeline görseli, VZA ve H3 görselleri, referans formatı, disclaimer banner'ı (C5) |
| `TASARIM_EXPORT_BRIFING.md` | Export butonu ve format seçimi, acil müdahale brifingi metrikleri, yazdırma görünümü (C6) |
| `TASARIM_BELIRSIZLIK_RENK.md` | Grafik türüne göre belirsizlik gösterimi, erişilebilir renk skalaları, palet seçici (C7) |
| `RENK_DENETIMI.md` | `tools/cvd_check.py` ile üretilen renk körlüğü / kontrast denetim raporu (elle düzenlenmez) |
| `VAKA_CIN_ANIZ.md` | C11 vaka çalışması: Çin 2013–2018 anız yakma yasağı, ham vs harmonize seri, boyut/mevsim kayması, sunum metni; veri ve grafikler `vaka/` altında, `crates/firms-client/examples/china_ban_case.rs` ile üretilir |
| `SUREKLILIK_ANALIZI.md` | G11 sensör dikişleri (2002 Aqua, 2012 SNPP, 2018 NOAA-20) süreklilik referans ölçümü: 10 ülkede ham vs harmonize metrik analizi |
| `KOPRU_KATSAYILARI.md` | K13 S-NPP ↔ NOAA-20 sayım/FRP köprü katsayıları ve Terra `≤20 dk` VZA kontrolü: 6 ülkede arşiv ölçümü |
| `TAM_FILO_TREND.md` | K7 tam filo (MODIS + S-NPP + NOAA-20 + NOAA-21 area API ile) 2000–2025 trend ve 2012/2018/2023 seviye kayması ölçümü: 15 ülke, havuzlanmış ve ülke bazında; MODIS referansı ile düzeltilmiş karar ölçütü |
| `DOGRULAMA_PAKETI.md` | G12 iç doğrulama paketi referans ölçümü: 10 ülkede placebo adım testi, LOYO normalizasyon testi ve Model vs Baseline kapısı |
| `KONFORMAL_KAPSAMA.md` | S10 conformal tablosunun (`data/conformal.json`) kalibrasyonu ve ölçülen kapsaması (LOYO, LOCO, bootstrap), dürüst sınırlar; `conformal_calibration fit` üretir, elle düzenlenmez |
| `crates/firms-client/examples/event_recall.rs` | `DOGRULAMA_PAKETI.md` bölüm D: belgelenmiş yangın aylarının ham/harmonize z ve sıra ölçümü, 2012 sonrası alarm oranları |
| `DIS_DOGRULAMA_YANMIS_ALAN.md` | Y10 dış doğrulama pilotu: ESA Fire_cci 0.25° yanmış alanına karşı ham vs harmonize Lin's CCC ve hücre-ay IOE/ICE, 4 bölge 2012–2020; `burned_area_validation.rs` + `tools/firecci_extract.py`, veri `vaka/firecci_validation_*` |
| `REFERENCES.md` | Bilimsel referans listesi taslağı: 32 makalelik envanter, referans projeler, doğrulanması gereken iddialar, pipeline aşaması → kaynak tablosu (S13, S8 içeriği için) |
| `test/TEST_PLANI.md` | IEEE 829 ana test planı: kapsam, seviyeler, giriş/çıkış ölçütleri, riskler, hata yönetimi, metrikler, geçerleme planı |
| `test/TEST_TASARIMI.md` | ID'li test vakaları: EP/BVA, karar tabloları, durum geçişi (0-switch), geçerleme vakaları (`TC-VAL`); otomatik testi olmayanlar `EKSİK` |
| `test/IZLENEBILIRLIK.md` | Gereksinim → test vakası → otomatik test izlenebilirlik matrisi ve risk sırasına göre boşluk özeti |
| `test/CI_SURE_VE_GOREV_BOLUSUMU.md` | CI'ın en çok 30 dk'da bitmesi için süre bütçesi, ölçülenler ve ölçülmeyenler, iki kişiye eşit bölünmüş iş paketleri (A: Rust hattı, B: web ve tetikleyiciler), ölçüm tablosu |

## Doküman Ekleme Kuralları

- Her doküman tek bir konuyu kapsamalıdır.
- Mermaid diyagramları tercih edilir (GitHub render desteği var).
- Kod örnekleri güncel tutulmalıdır — eski API referansları temizlenir.
