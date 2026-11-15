# G6 — Final entegrasyon ve teslim paketi

Doğrulama: 15 Kasım 2026. Hazır sonuçların üretim commit'i: `9beec6d`.

## Çalıştırma

Docker ile backend ve derlenmiş React arayüzü aynı Axum sürecinde sunulur:

```bash
cp .env.example .env
# Canlı FIRMS verisi için .env içine FIRMS_API_KEY yazılır.
docker compose up -d --build
# http://localhost:3001
make smoke
```

Tarihsel harita, takvim ve seri uçları yerel FIRMS arşivini gerektirir. Arşiv bu makinede hazır; başka makinede bir kez `make data` ve `make verify-data` çalıştırılır. Yaklaşık 41 GB ham arşiv teslim ZIP'ine konmaz. 238 bölgenin türetilmiş sonuçları pakette `data/derived` altında gelir.

Rust ve Node kuruluysa `npm ci --prefix web`, `npm run build --prefix web` ve ardından proje kökünde `cargo run -p pyrosync-server` yeterlidir. Bu yol `.env` dosyasını kendiliğinden yüklemez; canlı FIRMS anahtarı ortam değişkeni olarak verilmelidir. Anahtarsız sunucu açılır, arşiv ve arayüz çalışır, canlı uçlar 503 döner.

## Üretilen ve doğrulanan veriler

- Önceki tam üretim: 238/238 bölge, 0 hata, 7857,67 saniye; 476 seri/metadata ve iki kapsam dosyası.
- Docker'daki güncel sürümle `precompute` tekrar çalıştırıldı: 238 geçerli sonuç atlandı, 0 hata, 15,92 saniye. Gereksiz yeniden hesaplama yapılmadı.
- Şema: `10`; config özeti: `dd7b6c98ca9b9963`. Tüm JSON gövdelerinin sağlama toplamları ve 10.095 kaynak arşiv dosyasının boyutları doğrulandı.
- Conformal tablo etkin config ile uyumlu; hem yerel sunucu hem Docker başlangıcında 12 kayıt yüklendi.

## Kabul kontrolleri

| Kontrol | Sonuç |
|---|---|
| `docker compose build` ve `docker compose up -d --no-build` | Başarılı; konteynerde backend + statik frontend |
| Konteyner içinden 14 smoke kontrolü | 14 PASS, 0 FAIL, 0 SKIP; canlı FIRMS isteği HTTP 200 |
| `cargo run -p pyrosync-server --locked`, ayrı 3016 portu | Başarılı; aynı süreç `web/dist` ve API'yi sunuyor |
| Boş yazılabilir derived cache ile yerel smoke | 14 PASS; seriler ve kapsam `bundled` kaynaktan, anahtarsız canlı uç 503 |
| `cargo test --workspace --locked` | 737 geçti, 2 bilinen test ignore |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Başarılı |
| Frontend typecheck, test ve production build | Başarılı; 76 test geçti |
| GitHub Actions | G13 raporunda bağlantılı üç ardışık yeşil koşu; en uzun job 120 saniye |

## Teslim paketi

Yerel çıktı: `.cache/release/pyrosync-submission.zip`. Kaynak dosyalar, gerçek Git LFS görselleri, derlenmiş `web/dist`, örnek ortam dosyası, hazır sonuçlar ve kurulum belgeleri içerir. `RELEASE_MANIFEST.json`, kaynak commit'ini, çalışma ağacındaki farkları ve her dosyanın SHA-256 değerini kaydeder. ZIP bütünlüğü ve içeriği ayrıca doğrulanır.

`.env`, API anahtarları, ham arşiv, yerel önbellekler, `node_modules`, `target` ve araştırma klasörleri pakete alınmaz. Docker kurulumu ZIP içindeki kaynaklardan yeniden build edilebilir.

G5 sunum/video ve yarışma portalına gönderim ayrı işlerdir. Kullanıcının G6'yı şimdi tamamlama talebiyle teknik entegrasyon ve yerel deployment, G5'i beklemeden doğrulanmıştır. Bilinen BUG-8 (uzun yaşayan süreçte FIRMS availability bilgisinin yenilenmesi) önceki G6 notunda açıkça kayıtlıdır; bu kontrol koşusunda yeni bir engel oluşturmadı.
