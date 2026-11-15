# docs — Proje Dokümantasyonu

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- Teknik dokümantasyon Türkçe yazılır (MIMARI.md, API referans vb.).
- README.md İngilizce kalır (jüri değerlendirmesi için).

## Mevcut Dokümanlar

| Dosya | İçerik |
|-------|--------|
| `MIMARI.md` | Proje mimarisi, dizin yapısı, pipeline açıklaması, tech stack |
| `UI-UX-KONSEPT.md` | UI/UX konsepti, wireframe'ler, kullanıcı akışı, bileşen ağacı (C1) |
| `TASARIM_KONSEPT.md` | UI/UX tasarım konsepti, geçiş koreografisi, yerleşim ve ekran listesi |
| `UX_YAZIM.md` | Ses tonu, mikro metin kuralları, EN/TR metin tablosu ve biçim standartları |
| `FRONTEND_STACK_ONERISI.md` | Frontend teknoloji yığını kararları, uygulanan prototip seçimi ve API sözleşmesi |
| `TASARIM_METHODS.md` | Methods sayfası/çekmecesi, 7 aşamalı pipeline görseli, VZA ve H3 görselleri, referans formatı, disclaimer banner'ı (C5) |
| `TASARIM_EXPORT_BRIFING.md` | Export butonu ve format seçimi, acil müdahale brifingi metrikleri, yazdırma görünümü (C6) |
| `TASARIM_BELIRSIZLIK_RENK.md` | Grafik türüne göre belirsizlik gösterimi, erişilebilir renk skalaları, palet seçici (C7) |
| `RENK_DENETIMI.md` | `tools/cvd_check.py` ile üretilen renk körlüğü / kontrast denetim raporu (elle düzenlenmez) |

## Doküman Ekleme Kuralları

- Her doküman tek bir konuyu kapsamalıdır.
- Mermaid diyagramları tercih edilir (GitHub render desteği var).
- Kod örnekleri güncel tutulmalıdır — eski API referansları temizlenir.
