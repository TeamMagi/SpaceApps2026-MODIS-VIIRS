# CI süre bütçesi (en çok 30 dk) ve iki kişilik iş bölüşümü

Durum tarihi: 2026-11-15. Kaynak: `.github/workflows/ci.yml` (7bfdb17, 7b6ea22), yerel ölçümler (Docker `pyrosync-dev`, Rust 1.100).

## 1. Hedef

CI'daki bütün testler ve denetimler, bir push'tan sonra **en çok 30 dakikada** bitmeli. Süre, birbirine paralel çalışan job'ların en uzununa eşittir (job'lar paralel çalışır, toplanmaz).

Bu belge iki şeyi verir: şu anki durumun dürüst özeti (ne ölçüldü, ne ölçülmedi) ve bu hedefi sağlamak için iki kişiye **eşit** bölünmüş, birbirini beklemeyen iki iş paketi.

## 2. Mevcut hat

| Job | Adımlar | `timeout-minutes` | Not |
|-----|---------|-------------------|-----|
| `rust` | `cargo check`, `cargo clippy -D warnings`, `cargo test` | 30 | Rust 1.100'e sabit |
| `coverage` | `cargo llvm-cov` (satır kapısı %90), özet, `lcov.info` yükleme | **40** | Hedefin (30 dk) üstünde |
| `web` | `npm ci`, `typecheck`, `npm test`, `npm run build` | 15 | Node 20, npm önbelleği açık |

Tetikleyiciler: `main`'e push, `main`'e pull request, elle (`workflow_dispatch`). Aynı dal için eski koşu iptal edilir (`cancel-in-progress`).

## 3. Neyi biliyoruz (ölçüldü)

Ölçümler yerel Docker'da, önbellek ısınmışken, **yalnız test yürütme süresidir (derleme hariç)**:

| Ölçüm | Süre |
|-------|------|
| `pyrosync-core` testleri (368 test) | yaklaşık 8–10 sn |
| `pyrosync-firms` testleri (143 + 9 + 5 test) | yaklaşık 1–2 sn |
| `pyrosync-server` testleri (212 test) | yaklaşık 3,5–5,5 sn |
| web `npm test` (63 test) | yaklaşık 6 sn |
| web `npm run build` | yaklaşık 2,5–3,5 sn |

Sonuç: bütün testlerin yürütülmesi toplam yarım dakikanın altında. **Süreyi test kodu değil derleme belirler.** Rust işleri iki ayrı derleme yapar (normal ve `llvm-cov` için işaretlenmiş), her ikisi de önbellek yoksa tüm bağımlılıkları sıfırdan derler.

Başka bilinenler: satır kapsamı %95,5 (kapı %90); 737 Rust testi (2'si `#[ignore]`), 63 web testi; clippy temiz.

## 4. Neyi bilmiyoruz (ölçülmedi)

- **GitHub'daki gerçek süreler.** Depo özel ve bu oturumda `gh` girişi yok, Actions sonucuna bakılamadı. Soğuk önbellekli ilk koşunun ve ısınmış koşunun gerçek süresi bilinmiyor.
- GitHub'ın standart runner'ı (özel depolarda genelde 2 vCPU) yerel makineden yavaş olabilir. Bu yüzden aşağıdaki hedefler tahmin değil **ölçülüp doldurulacak** kabul ölçütleridir.
- Soğuk derleme süresi yerelde ayrıca ölçülmedi (Docker belleği paylaşımlı olduğundan sayı yanıltıcı olurdu).

## 5. İki eşit iş paketi

İki paket farklı bloklara dokunur, birbirini beklemez. Tahmini iş yükü eşittir (yaklaşık 55 dk her biri). İsimler doldurulacak: **Kişi A** = ____ , **Kişi B** = ____.

Çakışmayı önlemek için dosya sahipliği: ikisi de `.github/workflows/ci.yml` dosyasında çalışır, ama **A yalnız `rust` ve `coverage` bloklarına, B yalnız `on:` bloğuna ve `web` bloğuna** dokunur. Her değişiklikten önce `git pull --rebase`, küçük commit, hemen push.

### Paket A: Rust hattı (yaklaşık 55 dk)

| # | İş | Süre | Kabul ölçütü |
|---|----|------|--------------|
| A1 | `cargo check` adımını kaldır. `clippy --all-targets` aynı hedefleri derler, check tekrar derleme demektir | 5 dk | `ci.yml`'de `cargo check` yok; clippy ve test adımları duruyor; `actionlint` temiz |
| A2 | `Swatinem/rust-cache@v2` için iki job'a ayrı `key` ver (`rust-test`, `rust-coverage`) ve `cache-on-failure: true` ekle. `llvm-cov` farklı `RUSTFLAGS` ile derlediği için target dizini paylaşılamaz, ayrı anahtar şart | 15 dk | İkinci koşuda "cache restored" görünür ve derleme süresi belirgin düşer |
| A3 | Çift test yürütmesi kararı: `rust` job'ındaki `cargo test` ile `coverage` job'ı aynı testleri iki kez derleyip koşturur. İlk iki gerçek koşuya bak. `coverage` toplamı 20 dk'dan uzunsa `rust` job'ından `cargo test`'i kaldır (testler zaten kapsam altında koşar, kırmızı olursa hata aynen görünür). 20 dk veya altındaysa ayrımı koru (hata kaynağı daha net) | 20 dk | Karar ve gerekçe §6 tablosunun altına yazılı |
| A4 | Ölçüm: ilk 3 koşunun adım adım sürelerini §6 tablosuna doldur (soğuk/ısınmış ayrımıyla), en yavaş adımı belirt | 15 dk | §6 Rust satırları dolu |

### Paket B: Web, tetikleyiciler ve doğrulama (yaklaşık 55 dk)

| # | İş | Süre | Kabul ölçütü |
|---|----|------|--------------|
| B1 | Web job'ını ölç: `npm` önbelleği isabet ediyor mu, `npm ci`, `typecheck`, `test`, `build` süreleri. Toplam 5 dk altı beklenir | 10 dk | §6 web satırı dolu |
| B2 | Süre bütçesini zorla: bütün job'larda `timeout-minutes` en çok 30 olsun (`coverage` şu an 40 → 30). 30 dk'yı aşan koşu zaman aşımıyla kırmızı olur | 10 dk | `ci.yml`'de en büyük `timeout-minutes` 30; `actionlint` temiz |
| B3 | Yol filtresi: yalnız belge değişen push ve PR'larda CI'ı atla (`paths-ignore`: `docs/**`, `**/*.md`, `GOREVLER.md`). **`data/**` ve `web/**` filtreye girmez**: web `web/src/lib/caseStudy.ts` ile `data/case-study/turkey/stats.json` dosyasını içe aktarır, bu dosya değişince web job'ı atlanmamalı | 20 dk | Yalnız `docs/` değişen test commit'inde CI çalışmaz; `crates/` ya da `data/` değişince çalışır |
| B4 | "Kırmızı CI" sorun giderme tablosunu yaz (§7): en sık nedenler ve ilk bakılacak yer | 15 dk | §7 tablosu repoda; her satır gerçek bir nedene dayanır |

### Birlikte bitiş ölçütü

- Üst üste **3 koşuda** toplam süre (en uzun job) ≤ 30 dk. İlk koşu soğuk önbellekli olabilir, sayılır.
- §6 tablosu dolu, A3 kararı yazılı.
- Bir koşu yeşil görüldüğünde GOREVLER'de S5 kutusu `[X]` yapılır.

## 6. Ölçüm tablosu (doldurulacak)

Kaynak: GitHub → Actions → koşu → her job'ın süresi. `gh` girişi varsa: `gh run list --workflow CI --limit 5` ve `gh run view <id> --json jobs`.

| Koşu (commit) | Önbellek | `rust` | `coverage` | `web` | Toplam (en uzun) | Sonuç |
|---------------|----------|--------|------------|-------|------------------|-------|
| 1. koşu | soğuk | ? | ? | ? | ? | ? |
| 2. koşu | ısınmış | ? | ? | ? | ? | ? |
| 3. koşu | ısınmış | ? | ? | ? | ? | ? |

A3 kararı: ____

## 7. Kırmızı CI'da ilk bakılacak yerler (B4 taslağı)

Aşağıdakiler bu projede gerçekten görülen ya da bilinen nedenlerdir. B kişisi doğrular ve tamamlar.

| Belirti | Olası neden | Ne yapılır |
|---------|-------------|------------|
| `clippy` kırmızı, kod değişmemiş | Yeni Rust sürümü yeni lint getirdi | Araç zinciri 1.100'e sabit (`ci.yml`); sürümü Dockerfile ile birlikte yükselt |
| `cargo llvm-cov` kapıda kırmızı | Satır kapsamı %90'ın altına indi | Yeni kod için test ekle; kapıyı düşürme |
| web `npm test` kırmızı, süre mesajı | `web/tests/bench.test.ts` zaman eşiği yavaş runner'da aşıldı | Eşik gerçekten aşılıyorsa runner'a göre ayarla; yerelde ≈ 600 ms / 33 ms ölçüldü |
| `npm ci` hata veriyor | `package.json` ile `package-lock.json` uyuşmuyor | Kilit dosyasını güncelle ve commit'le |
| Koşu 30 dk'da kesiliyor | Önbellek yok veya derleme çok uzun | §6 tablosuna bak; A2 ve A3 |
| Testler yerelde geçiyor, CI'da geçmiyor | Zaman ya da ortam bağımlılığı (saat, gitignored dosya) | F.I.R.S.T. ihlali mi bak: `crates/server/src/integration_tests.rs` içindeki `days=` yolları gerçek UTC saatini kullanır (gece yarısı koruması var), diğer testler sabit gün kullanır |

## 8. Bu kapsama girmeyenler (bilerek)

- `make smoke` CI'a alınmaz: 41 GB'lık FIRMS arşivi repoda yok ve CI'da yok; yerelde ve G6 final adımında koşulur.
- Dal kapsamı (`--branch`) kapılanmaz: kararlı Rust'ta yok, nightly gerekir.
- `data/derived` paketinin yenilenmesi CI işi değildir (G6, Görkem).
