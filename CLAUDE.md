# PyroSync — Proje Kuralları

## Kesin Kurallar

### Git Commit Kuralları
- **Commit mesajlarına kesinlikle `Co-Authored-By` satırı EKLENMEYECEKTİR.**
- **Commit mesajlarında Claude, AI, LLM veya herhangi bir yapay zeka imzası/referansı bulunmayacaktır.**
- Commit mesajları Türkçe veya İngilizce olabilir, kısa ve açıklayıcı olmalıdır.
- Tek branch (`main`) üzerinden sırayla commit atılır.
- Her commit tek bir mantıksal değişikliği kapsamalıdır.

### Git Push Kuralları
- **Remote kontrol edilmeden push YAPILAMAZ.** Her push'tan önce mutlaka `git pull` çalıştırılır.
- `git pull` sonucunda **herhangi bir çakışma (conflict) veya ezme (overwrite) riski varsa, push yapılmaz.** Önce çakışmalar çözülür, çözüm doğrulanır, ancak ondan sonra push edilebilir.
- Çakışma çözümünde karşı tarafın değişikliği körü körüne ezilmez; her iki tarafın niyeti korunacak şekilde birleştirilir. Hangi değişikliğin neden tercih edildiği kullanıcıya bildirilir.
- Çakışma çözümünden sonra push öncesi `make test` (veya en az `cargo test --workspace`) tekrar koşulur; testler geçmeden push edilmez.
- `git push --force` ve `--force-with-lease` **kullanıcı açıkça istemediği sürece kullanılmaz.**

### Dil ve Derleyici Kuralları
- **Backend mutlaka Rust ile yazılır.** Başka backend dili kabul edilmez.
- **TypeScript derleyicisi olarak SWC kullanılır** (`@vitejs/plugin-react-swc`). Babel kullanılmaz.
- Rust edition: **2024**, workspace resolver: **2**.
- Frontend: React 19 + TypeScript strict mode.

### Görev Takibi Kuralları
- Projenin **tek** görev listesi `GOREVLER.md`'dir. Ayrı görev dosyası (GOREVLER2.md vb.) açılmaz; yeni görevler ilgili kişinin bölümüne eklenir.
- Her görev başlığının başında onay kutusu bulunur: `[ ]` = yapılmadı, `[X]` = tamamlandı (ör. `### [ ] G5 — ...`).
- Görevi tamamlayan kişi, "Bitti sayılır" şartı sağlandığında kendi görevinin kutusunu (başlıkta ve özet tablosunda) `[X]` yapar. Şart sağlanmadan `[X]` konmaz.
- Görev metinleri silinmez veya kısaltılmaz; kapanış ve durum bilgisi görevin altına not olarak eklenir.

### Hazır Sonuçlar (data/derived) Kuralı
- Harmonizasyon davranışını veya ayarlarını (`PipelineConfig`, dedup/fleet/matching/VZA/kalibrasyon/belirsizlik/maske mantığı) ya da `/api/series` şemasını değiştiren commit'ler `DERIVED_SCHEMA_VERSION`'ı artırır (sürüm geçmişi `crates/server/src/derived_cache.rs` içinde yorum olarak tutulur); bu, repodaki hazır sonuçları geçersiz kılar.
- **Ekip kararı (2026-11-15):** Geliştirme sürerken her değişiklikte `make precompute` (~1,5 saat) çalıştırılmaz; `data/derived` final entegrasyona (G6) kadar bayat kalabilir ve config değişiklikleri bunu beklemeden `main`'e alınır. Final adımında arşivi olan tek bir kişi `make precompute` → `make bundle-derived` çalıştırır ve `data/derived`'ı commit'ler. Arada başka biri `data/derived` üretip commit'lemez.
- Bu ara dönemde `make smoke`'un 3. kontrolü ("238 precomputed") beklenen şekilde FAIL verir; diğer kontroller geçmelidir. Final adımından sonra 14 kontrolün tamamı PASS olmalıdır.

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

## Agent Orkestrasyon (Zorunlu)

**Her konuşmada bu kurallar otomatik geçerlidir.** Detaylar: `.claude/skills/orchestrator/SKILL.md`

### Rol Dağılımı
- **Sen (Claude Code):** Planlama, görev parçalama, prompt yazma, review, birleştirme. **Asla bizzat kod üretme** — kod üretimini her zaman bir alt ajana delege et.
- **Codex CLI (`/codex`, ChatGPT üyeliği):** Kod ve dosya yazan birincil ajan; `-s workspace-write` ile headless modda doğrudan repoya yazar. Modeller: `gpt-6-luna` (Seviye 1, hızlı), `gpt-6.1-sol` (Seviye 2–3, ana işgücü), `gpt-6-astra` (çok zorlu Seviye 3 ve Seviye 4, kotayı koru). Modeli her zaman `-m` ile açıkça ver.
- **Gemini 4 Hydrogen (High):** Geniş context okuma, multimodal/görsel analiz, UI/UX, kod review. `/gemini` skill'i ile çağır (Antigravity CLI). **Headless modda dosya yazamaz** — kod gerekirse yalnız patch metni istenir.
- **Gemini 4 Argon (High):** Opus seviye muhakeme. `/gemini-argon` skill'i ile çağır. **KISITLI KOTA** — sadece Seviye 4 mimari konseyde.
- **Claude Haiku / Sonnet (dahili):** Yalnız dış servisler (Codex, Antigravity) hata verdiğinde acil yedek.

### Görev Seviyeleri
1. **Basit işler** (regex, boilerplate, format, dokümantasyon) → `/codex` `gpt-6-luna`
2. **Orta backend** (API, CRUD, test, algoritma) → `/codex` `gpt-6.1-sol` yazar; geniş context analizi ve review için `/gemini`
3. **Üst seviye / görsel / UI** (entegrasyon, multimodal, UI/UX) → `/gemini` analiz/görsel doğrulama + `/codex` `gpt-6.1-sol` (çok zorsa `gpt-6-astra`) yazım
4. **Mimari konsey** (riskli/belirsiz kararlar) → `/gemini-argon` + `/codex` `gpt-6-astra` paralel → senin sentezin

### Fallback Zinciri
Dış servis hata verirse (429, kota/usage limit, timeout, boş yanıt):
- Tier 1: Codex `gpt-6-luna` → Claude Haiku alt ajanı
- Tier 2: Codex `gpt-6.1-sol` → Hydrogen (patch metni) → Haiku (rutin) veya Sonnet (karmaşık)
- Tier 3: Codex → Claude Sonnet (kod yazımı); Hydrogen → Claude Sonnet (analiz/görsel)
- Tier 4: Yanıt veremeyen modelin (Argon veya `gpt-6-astra`) yerine "Eleştirel Sistem Mimarı" rolüyle Claude Sonnet

### Devre Kesici
Kota/rate-limit hatası alan servis oturum boyunca `EXHAUSTED` işaretlenir, tekrar istek atılmaz (Codex'in üç modeli aynı üyelik kotasını paylaşır). Doğrudan fallback rotasına geçilir. Kullanıcıya tek satır bilgi verilir. Hydrogen'ın dosya yazamaması kota hatası değil kalıcı sınırdır; `--dangerously-*` bayraklarıyla aşılmaz.
