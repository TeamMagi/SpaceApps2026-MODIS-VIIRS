# CI süre bütçesi (en çok 30 dk): ölçüm, bulgular ve iş bölüşümü

Durum tarihi: 2026-11-15. İlk prova tabanı: `main` `a1e37ff`. Yerel soğuk provanın ardından GitHub Actions API ve job loglarıyla üç ardışık yeşil koşu doğrulandı; gerçek süreler §6'da.

## 1. Sonuç

**G13 tamamlandı:** Üç ardışık gerçek koşuda `rust` / `coverage` sırasıyla 92/91, 115/88 ve 120/89 saniye. Her iki job üç koşuda da 30 dakika sınırının altında ve yeşil. Ayrı Rust önbelleklerinin tam isabetle geri yüklendiği loglarda görüldü. Aşağıdaki yerel ölçümler tarihsel prova bulgularıdır.

1. **Süre sorun değil.** Soğuk önbellekle en uzun job 158 sn (yaklaşık 2,6 dk). 30 dk hedefine uzaklık büyük.
2. **Asıl sorun kırmızı adımlardı.** Prova, `main`'deki CI'ın web job'ında **dört ayrı yerden** kırılacağını gösterdi (§4). Yerelde fark edilmedi, çünkü geliştirici makinesinde Node 24, CI'da Node 20 vardı.
3. Dördü de giderildi: 1, 2 ve 4'ü Serhat'ın `a1e37ff` onarımı (22:40) çözdü, 3'ü bu push'taki `ci.yml` değişikliği (web job'ı Node 22) çözüyor. Yeni `main` üzerinde provada yeşil.

## 2. Prova yöntemi

Her job ayrı bir Docker kabında, aynı anda çalıştırıldı: `--cpus 2 --memory 7g` (GitHub'ın özel depolardaki standart runner'ına benzer sınır), boş `target` ve boş cargo kayıt dizini (soğuk önbellek, en kötü durum), Rust 1.100. Web job'ı Linux üzerinde `node:20` ve `node:22` ile. Komutlar `ci.yml` ile aynı: `clippy -D warnings`, `cargo test`, `cargo llvm-cov --fail-under-lines 90`, `npm ci`, `typecheck`, `test`, `build`.

## 3. Ölçüm (soğuk önbellek, yerel)

| Job | Adım | Süre | Sonuç |
|-----|------|------|-------|
| `rust` | `clippy --all-targets -D warnings` | 55 sn | geçti |
| `rust` | `cargo test --workspace` | 103 sn | geçti |
| `rust` | **toplam** | **158 sn** | geçti |
| `coverage` | `cargo llvm-cov` + kapı (%90) | 107 sn | geçti, satır kapsamı %95,5 |
| `web` (Node 22, düzeltmelerle) | `npm ci` 7, `typecheck` 5, `test` 4, `build` 4 | 23 sn | geçti (76 test) |

Üç job paralel çalışır; CI süresi en uzununa eşittir: **yaklaşık 158 sn**. GitHub'da araç zinciri kurulumu, önbellek geri yükleme ve daha yavaş çekirdekler eklenir; yine de 30 dk'ya (1800 sn) yaklaşık 11 kat pay var.

**GitHub'daki gerçek süreler:** Üç ardışık yeşil koşuda en uzun job 92–120 saniye (§6). Rust ve coverage önbellekleri üçünde de tam isabetle geri yüklendi. Bu bölümdeki soğuk ölçümler yerel provadır.

## 4. Provada bulunan kırmızılar (düzeltmeden önce)

| # | Bulgu | Neden | CI'da kırılan adım | Düzeltme |
|---|-------|-------|--------------------|------------------|
| 1 | `copy.ts` içinde `methodsSources` anahtarı iki kez tanımlı (EN ve TR) | `d8fa747` ("Redesign frontend", 21:47) eski S-NPP metnini yeniden getirdi; NOAA-20'li sürümle birleşince kopya oluştu | `npm run typecheck` (TS1117) | Serhat'ın `a1e37ff` onarımı |
| 2 | `BottomBar.tsx` `t("disclaimerLink")` çağırıyor ama anahtar silinmiş | Aynı commit anahtarı `copy.ts`'ten çıkarmış, bileşeni güncellememiş. Arayüzde bağlantı metni de eksik kalıyordu | `npm run typecheck` (TS2554) | Serhat'ın `a1e37ff` onarımı |
| 3 | `npm test` Node 20'de hiç çalışmıyor | Betik `node --test ".test-out/*.mjs"` tırnaklı glob kullanıyor; Node 20 bunu genişletmez (Node 21+ özelliği). CI Node 20 kullanıyordu | `npm test` | CI web job'ı Node 22'ye alındı (bu push) |
| 4 | `validationGate.test.ts` eski metni bekliyor | Aynı yeniden tasarım `validationGateFail` metnini "Does not beat baseline · interpret with care" yaptı, test birebir eski metni arıyordu | `npm test` (1 başarısız test) | Serhat'ın `a1e37ff` onarımı |

Not: 3. bulgu `main`'e özgü değil, CI kurulumundan gelen yapısal bir sorundu; 1, 2 ve 4 `main`'deki bir commit'in (`d8fa747`) kırdığı şeyler. Aynı kırığı ben de yerelde düzeltmiştim, ama Serhat 22:40'ta aynı dosyalarda onarım yaptığı için kendi sürümümü bırakıp onunkini aldım; çakışma çıkmadı.

## 5. Bu push'taki değişiklikler

| Dosya | Değişiklik |
|-------|-----------|
| `.github/workflows/ci.yml` | `cargo check` adımı kaldırıldı (clippy zaten derler); iki `rust-cache` adımına `cache-on-failure: true`; `coverage` `timeout-minutes` 40 → 30; `push` ve `pull_request` için `paths-ignore` (`docs/**`, `**/*.md`, `GOREVLER.md`; `data/**` ve `web/**` dışarıda, çünkü web `data/case-study/turkey/stats.json` dosyasını içe aktarır); web job'ı Node 20 → 22 |

`actionlint` temiz. rust-cache anahtarının varsayılan olarak job kimliğini içerdiği gerçek koşu loglarında da doğrulandı (§6).

## 6. Gerçek ölçüm ve G13 kapanışı

Kaynak: GitHub Actions REST API `actions/runs/{id}/jobs` ve aynı koşuların indirilen job logları. Job süresi `completed_at - started_at`; toplam sütunu en uzun job'dır, kuyruk süresini içermez. Üç koşu da `ubuntu-24.04`, Rust 1.100 ve Node 22 kullandı.

| Koşu (commit) | Rust önbelleği | `rust` | `coverage` | `web` | En uzun job | Sonuç |
|---|---|---|---|---|---|---|
| [#65](https://github.com/TeamMagi/SpaceApps2026-MODIS-VIIRS/actions/runs/43756377651) (`bba155f`, push) | İkisinde de tam isabet | 92 sn | 91 sn | 41 sn | 92 sn | Yeşil |
| [#66](https://github.com/TeamMagi/SpaceApps2026-MODIS-VIIRS/actions/runs/43757124772) (`ddd4ba6`, push) | İkisinde de tam isabet | 115 sn | 88 sn | 29 sn | 115 sn | Yeşil |
| [#67](https://github.com/TeamMagi/SpaceApps2026-MODIS-VIIRS/actions/runs/43757588852) (`d7a9903`, workflow_dispatch) | İkisinde de tam isabet | 120 sn | 89 sn | 36 sn | 120 sn | Yeşil |

**Önbellek kanıtı (A2):** Üç koşunun tamamında `Cache hit for`, `Cache restored successfully` ve `full match: true` görüldü. `rust` anahtarı `v0-rust-rust-Linux-x64-609f6319-3f7afb44`; `coverage` anahtarı `v0-rust-coverage-Linux-x64-609f6319-3f7afb44`. Ayrım job kimliğinden otomatik geliyor; görevde örnek verilen ek `key: rust-test/rust-coverage` girdileri aynı ayrımı tekrar edeceği için eklenmedi. `cache-on-failure: true` her iki job'da etkin.

Üç başarılı koşu da ısınmış önbellek kullandı; soğuk koşu diye etiketlenmedi. Soğuk derleme kanıtı §3 ve §9'daki yerel ölçümlerdir. Önceki kırmızı #63 ve #64, bu üç ardışık başarılı koşudan önceydi ve kapanış dizisine dahil edilmedi.

**A3 kararı: çift test yürütmesi korunur.** İlk iki gerçek koşuda coverage toplamı 91 ve 88 saniye; görevin 20 dakika eşiğinin çok altında. Clippy/test ile coverage ayrı kalınca hata kaynağı açık kalır. En uzun job 120 saniye; 30 dakika sınırına 1680 saniye pay var.

A1 (`cargo check` kaldırıldı), A2 (ayrı ve geri yüklenen önbellekler), A3 (gerçek ölçüme dayalı karar) ve A4 (üç koşunun adım süreleri aşağıda) tamam. S14'ün belge filtresi doğrulaması ayrı görevdir.

### Adım süreleri (saniye)

| Job | Adım | #65 | #66 | #67 |
|---|---|---|---|---|
| `rust` | Set up job | 1 | 1 | 2 |
| `rust` | Run actions/checkout@v4 | 3 | 3 | 2 |
| `rust` | Run dtolnay/rust-toolchain@master | 10 | 10 | 10 |
| `rust` | Run Swatinem/rust-cache@v2 | 13 | 10 | 9 |
| `rust` | cargo clippy | 11 | 11 | 14 |
| `rust` | cargo test | 51 | 76 | 79 |
| `rust` | Post Run Swatinem/rust-cache@v2 | 0 | 0 | 0 |
| `rust` | Post Run actions/checkout@v4 | 0 | 0 | 1 |
| `rust` | Complete job | 0 | 1 | 0 |
| `coverage` | Set up job | 2 | 2 | 2 |
| `coverage` | Run actions/checkout@v4 | 3 | 3 | 2 |
| `coverage` | Run dtolnay/rust-toolchain@master | 11 | 13 | 12 |
| `coverage` | Run Swatinem/rust-cache@v2 | 8 | 9 | 6 |
| `coverage` | Run taiki-e/install-action@cargo-llvm-cov | 1 | 1 | 2 |
| `coverage` | Coverage gate | 60 | 54 | 61 |
| `coverage` | Coverage summary | 0 | 1 | 0 |
| `coverage` | Run actions/upload-artifact@v4 | 2 | 1 | 1 |
| `coverage` | Post Run Swatinem/rust-cache@v2 | 0 | 0 | 0 |
| `coverage` | Post Run actions/checkout@v4 | 0 | 1 | 1 |
| `coverage` | Complete job | 0 | 0 | 0 |
| `web` | Set up job | 1 | 1 | 1 |
| `web` | Run actions/checkout@v4 | 2 | 1 | 2 |
| `web` | Run actions/setup-node@v4 | 3 | 5 | 2 |
| `web` | Run npm ci | 4 | 3 | 3 |
| `web` | Run npm run typecheck | 12 | 7 | 10 |
| `web` | Run npm test | 7 | 4 | 7 |
| `web` | Run npm run build | 5 | 4 | 7 |
| `web` | Post Run actions/setup-node@v4 | 3 | 0 | 0 |
| `web` | Post Run actions/checkout@v4 | 0 | 1 | 1 |
| `web` | Complete job | 0 | 0 | 0 |

Adımların toplamı job süresinden farklı olabilir; runner hazırlığı ve adımlar arası süreler job toplamına dahildir. API zaman damgaları saniye çözünürlüğündedir.


## 7. Kırmızı CI'da ilk bakılacak yerler

| Belirti | Olası neden | Ne yapılır |
|---------|-------------|------------|
| Yerelde geçiyor, CI'da kırmızı | Yerel Node sürümü CI'dan yeni (yerelde 24, CI'da 22) ya da Windows/Linux farkı | Yerelde Docker'da `node:22-bookworm-slim` ile çalıştır |
| `typecheck` kırmızı | Birleştirmeden kalan kopya anahtar ya da silinmiş çeviri anahtarı | Hata satırındaki dosyayı aç; `copy.ts` anahtarlarını karşılaştır |
| `clippy` kırmızı, kod değişmemiş | Yeni Rust sürümü yeni lint getirdi | Araç zinciri 1.100'e sabit (`ci.yml`); Dockerfile ile birlikte yükselt |
| `cargo llvm-cov` kapıda kırmızı | Satır kapsamı %90'ın altına indi | Yeni kod için test ekle; kapıyı düşürme |
| web `npm test` kırmızı, süre mesajı | `web/tests/bench.test.ts` zaman eşiği yavaş runner'da aşıldı | Eşik gerçekten aşılıyorsa runner'a göre ayarla |
| `npm ci` hata veriyor | `package.json` ile `package-lock.json` uyuşmuyor | Kilit dosyasını güncelle ve commit'le |
| Koşu 30 dk'da kesiliyor | Önbellek yok ya da takılan adım | §6 tablosuna bak |
| Beklenen CI hiç çalışmadı | `paths-ignore`: yalnız belge değişmiş | Beklenen davranış; `workflow_dispatch` ile elle başlat |

## 8. Bu kapsama girmeyenler (bilerek)

- `make smoke` CI'a alınmaz: 41 GB'lık FIRMS arşivi CI'da yok; yerelde ve G6 final adımında koşulur.
- Dal kapsamı (`--branch`) kapılanmaz: kararlı Rust'ta yok, nightly gerekir.
- `data/derived` paketinin yenilenmesi CI işi değildir (G6, Görkem).

## 9. Derin analiz: temiz Linux klonu, tekrarlı koşu ve kapı sınaması (2026-11-15 22:58)

Yöntem: her job kendi konteynerinde, **temiz `git clone`** ile (LF satır sonu, Git LFS göstericileri, gitignore'lı dosya yok; CI'daki gibi) ve henüz commit'lenmemiş değişiklikler yama olarak uygulanarak; soğuk önbellek, 2 CPU / 7 GB. §3'teki ilk provanın zayıflıkları kapatıldı: önceki prova Windows'ta çıkarılmış (CRLF) klasörü kullanıyordu ve tek koşuydu.

| Job | Süre | Sonuç | Ek kontrol |
|-----|------|-------|-----------|
| `rust` | 234 sn (clippy 53, test 83, 5 tekrar, 2 olumsuz test) | geçti | 5 tekrarda 737/737 test, **0 kararsız koşu** |
| `coverage` | 100 sn (llvm-cov 95) | geçti, satır kapsamı %95,5 | %99 eşiği **reddedildi** (rc=1) |
| `web` (Node 22) | 58 sn | geçti | 5 tekrarda 76/76 test, **0 kararsız koşu** |

**Kapı sınaması (kasıtlı hata verip kırmızı çıkıyor mu):**

| Kapı | Verilen hata | Sonuç |
|------|-------------|-------|
| clippy `-D warnings` | `v.len() == 0` | kırmızı (rc=101, "length comparison to zero") |
| `cargo test` | `assert_eq!(1, 2)` | kırmızı (rc=101, test adı ve satır görünüyor) |
| kapsam kapısı | eşik %99 | kırmızı (rc=1) |
| web `typecheck` | `string` → `number` atama | kırmızı (rc=2, "TS2322") |

Bu, S5 "Bitti sayılır" ölçütünün ikinci yarısının ("başarısız build'de hata mesajı görünüyor") yerel karşılığıdır; GitHub'daki gösterimi ayrıca görülmeli.

**Doğrulananlar:** kaynak dosyalarda CR yok; LFS dosyaları gösterici olarak geliyor ve derleme ile testler bunlara bağlı değil; 737 Rust testi FIRMS arşivi, API anahtarı ve `.env` olmadan geçiyor; `web/package.json` içinde `engines` kısıtı yok, Node 22 uygun.

**Yerelde taklit edilemeyenler:** GitHub'daki gerçek koşu ve süre, `paths-ignore` davranışı, aksiyonların kurulum adımları (toolchain kurulumu, `rust-cache`, `install-action`). Bunlar için bağımsız bir ajan incelemesi yapıldı (aksiyon girdileri GitHub'daki gerçek tanımlara karşı); sonucu ayrıca eklenir.

Sonradan Serhat'ın `a1e37ff` onarımı geldi. Web job'ı bu yeni `main` üzerinde ve `ci.yml` değişikliğiyle (Node 22) yeniden koşuldu: `typecheck`, `test` (76/76, 5 tekrarda 0 kararsız koşu) ve `build` temiz; `typecheck` kapısı kasıtlı tür hatasında yine kırmızı verdi.
