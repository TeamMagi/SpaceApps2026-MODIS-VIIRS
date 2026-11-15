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
| `TAM_FILO_TREND.md` | K7 tam filo (MODIS + S-NPP + NOAA-20) 2000–2025 trend ve 2012/2018 seviye kayması ölçümü: 15 ülke, havuzlanmış ve ülke bazında |
| `DOGRULAMA_PAKETI.md` | G12 iç doğrulama paketi referans ölçümü: 10 ülkede placebo adım testi, LOYO normalizasyon testi ve Model vs Baseline kapısı |
| `crates/firms-client/examples/event_recall.rs` | `DOGRULAMA_PAKETI.md` bölüm D: belgelenmiş yangın aylarının ham/harmonize z ve sıra ölçümü, 2012 sonrası alarm oranları |
| `REFERENCES.md` | Bilimsel referans listesi taslağı: 32 makalelik envanter, referans projeler, doğrulanması gereken iddialar, pipeline aşaması → kaynak tablosu (S13, S8 içeriği için) |
| `test/TEST_PLANI.md` | IEEE 829 ana test planı: kapsam, seviyeler, giriş/çıkış ölçütleri, riskler, hata yönetimi, metrikler, geçerleme planı |
| `test/TEST_TASARIMI.md` | ID'li test vakaları: EP/BVA, karar tabloları, durum geçişi (0-switch), geçerleme vakaları (`TC-VAL`); otomatik testi olmayanlar `EKSİK` |
| `test/IZLENEBILIRLIK.md` | Gereksinim → test vakası → otomatik test izlenebilirlik matrisi ve risk sırasına göre boşluk özeti |

## Doküman Ekleme Kuralları

- Her doküman tek bir konuyu kapsamalıdır.
- Mermaid diyagramları tercih edilir (GitHub render desteği var).
- Kod örnekleri güncel tutulmalıdır — eski API referansları temizlenir.
