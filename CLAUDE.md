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
- Harmonizasyon davranışını veya ayarlarını (`PipelineConfig`, dedup/fleet/matching/VZA/kalibrasyon/belirsizlik/maske mantığı) ya da `DERIVED_SCHEMA_VERSION`'ı değiştiren her commit'ten sonra repodaki hazır sonuçlar geçersiz olur. Aynı iş kapsamında `make precompute` → `make bundle-derived` çalıştırılır ve `data/derived` commit'lenir.
- Push öncesi çalışan sunucuya karşı `make smoke` koşulur; 3. kontrol "238 precomputed" göstermiyorsa hazır sonuçlar bayattır.

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
- **DeepSeek V4.1 Flash (High):** Ana işgücü. `/deepseek` skill'i ile çağır (OpenCode CLI). Ücretsiz/sınırsız.
- **Gemini 4 Hydrogen (High):** Güçlü kodlama, geniş context, multimodal/görsel. `/gemini` skill'i ile çağır (Antigravity CLI).
- **Gemini 4 Argon (High):** Opus seviye muhakeme. `/gemini-argon` skill'i ile çağır. **KISITLI KOTA** — sadece kritik mimari kararlarda.

### Görev Seviyeleri
1. **Basit işler** (regex, boilerplate, format, dokümantasyon) → `/deepseek`
2. **Orta backend** (API, CRUD, test, algoritma) → `/deepseek`, geniş context gerekirse `/gemini`
3. **Üst seviye / görsel / UI** (entegrasyon, multimodal, UI/UX) → `/gemini`
4. **Mimari konsey** (riskli/belirsiz kararlar) → `/gemini-argon` + `/deepseek` paralel → senin sentezin

### Fallback Zinciri
Dış servis hata verirse (429, kota, timeout, boş yanıt):
- Tier 1: DeepSeek → Claude Haiku alt ajanı
- Tier 2: DeepSeek → Hydrogen → Haiku (rutin) veya Sonnet (karmaşık)
- Tier 3: Hydrogen → Claude Sonnet alt ajanı
- Tier 4: Yanıt veremeyen modelin yerine "Eleştirel Sistem Mimarı" rolüyle Claude Sonnet

### Devre Kesici
Kota/rate-limit hatası alan servis oturum boyunca `EXHAUSTED` işaretlenir, tekrar istek atılmaz. Doğrudan fallback rotasına geçilir. Kullanıcıya tek satır bilgi verilir.
