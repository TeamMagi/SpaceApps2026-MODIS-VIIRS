# PyroSync — Proje Kuralları

## Kesin Kurallar

### Git Commit Kuralları
- **Commit mesajlarına kesinlikle `Co-Authored-By` satırı EKLENMEYECEKTİR.**
- **Commit mesajlarında Claude, AI, LLM veya herhangi bir yapay zeka imzası/referansı bulunmayacaktır.**
- Commit mesajları Türkçe veya İngilizce olabilir, kısa ve açıklayıcı olmalıdır.
- Tek branch (`main`) üzerinden sırayla commit atılır.
- Her commit tek bir mantıksal değişikliği kapsamalıdır.

### Dil ve Derleyici Kuralları
- **Backend mutlaka Rust ile yazılır.** Başka backend dili kabul edilmez.
- **TypeScript derleyicisi olarak SWC kullanılır** (`@vitejs/plugin-react-swc`). Babel kullanılmaz.
- Rust edition: **2024**, workspace resolver: **2**.
- Frontend: React 19 + TypeScript strict mode.

### Güvenlik
- `.env` dosyaları commit edilmez, `.env.example` şablon olarak tutulur.
- API anahtarları environment variable üzerinden okunur, hiçbir zaman hard-code edilmez.
- `STRATEGY.md`, `CHALLENGE_INFO.md`, `reference-projects/`, `academic-papers/` dizinleri gitignore'dadır ve repoya eklenmez.

## Proje Yapısı

```
├── crates/core/           → Harmonizasyon motoru (bkz. crates/core/CLAUDE.md)
├── crates/firms-client/   → NASA FIRMS API istemcisi (bkz. crates/firms-client/CLAUDE.md)
├── crates/server/         → Axum web sunucusu (bkz. crates/server/CLAUDE.md)
├── web/                   → React frontend (bkz. web/CLAUDE.md)
├── docs/                  → Dokümantasyon (bkz. docs/CLAUDE.md)
├── tools/                 → Yardımcı scriptler (bkz. tools/CLAUDE.md)
```

## Build Komutları

```bash
make dev-backend      # Rust backend geliştirme sunucusu
make dev-frontend     # Vite dev server
make build            # Tam build (backend + frontend)
make test             # Tüm testler
make check            # Hızlı derleme kontrolü (cargo check)
make lint             # Clippy lint
make docker-up        # Docker ile başlat
make docker-down      # Docker durdur
```

## Geliştirme Ortamı

- Windows'ta GNU Rust toolchain kullanılır (`stable-x86_64-pc-windows-gnu`), MSVC değil.
- Node.js 20+, npm ile paket yönetimi.
- Docker multi-stage build ile on-premise dağıtım.

## Kod Standartları

- Yorum yazmadan önce düşün: kodun kendisi açıklayıcı olmalı. Yorum sadece "neden" sorusunu cevaplarken yazılır.
- Gereksiz abstraction yapılmaz. Basit ve direkt kod tercih edilir.
- Her crate'in kendi `CLAUDE.md` dosyası vardır, o modüle özgü kurallar orada belirtilir.
- Kullanılmayan dependency'ler ve import'lar temizlenir.
