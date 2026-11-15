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

## Doküman Ekleme Kuralları

- Her doküman tek bir konuyu kapsamalıdır.
- Mermaid diyagramları tercih edilir (GitHub render desteği var).
- Kod örnekleri güncel tutulmalıdır — eski API referansları temizlenir.
