# CI süre bütçesi (en çok 30 dk): ölçüm, bulgular ve iş bölüşümü

Durum tarihi: 2026-11-15, 22:30. Taban: `main` `a1e37ff`. Yöntem: CI'ın üç job'u yerelde, GitHub runner'ını taklit eden sınırlarla ve soğuk önbellekle çalıştırıldı.

## 1. Sonuç

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

**GitHub'daki gerçek süreler** (Actions, `main`): toplam 1 dk 36 sn ile 2 dk 15 sn (§6). Yerel provanın tahmin ettiği 158 sn'den kısa; önbellek durumu bilinmiyor. Yerel prova burada yalnız bir tahmindi, doğrulayan GitHub'daki koşulardır.

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

`actionlint` temiz. rust-cache anahtarının varsayılan olarak job kimliğini içerdiği (bu yüzden `rust` ve `coverage` ayrı önbellek kullanır) belgeye dayanır; ilk gerçek koşunun logunda doğrulanmalı.

## 6. Kalan iş (kişi başı yaklaşık 5 dk) ve gerçek ölçüm tablosu

| Kim | İş |
|-----|-----|
| Görkem (Paket A, G13) | İlk gerçek koşunun `rust` ve `coverage` sürelerini aşağıdaki tabloya yaz; "cache restored" logunu doğrula |
| Samet (Paket B, S14) | `web` süresini yaz; yalnız `docs/` değişen bir commit'te CI'ın çalışmadığını, `crates/` ya da `web/` değişince çalıştığını doğrula |

A3 kararı (çift test yürütmesi): **ayrım korunur.** `coverage` 107 sn, `rust` 158 sn; birleştirmenin kazancı önemsiz, ayrım hata kaynağını netleştirir.

| Koşu (commit) | Önbellek | `rust` | `coverage` | `web` | Toplam (en uzun) | Sonuç |
|---------------|----------|--------|------------|-------|------------------|-------|
| #63 (`85d40df`) | bilinmiyor | ? | ? | ? | 2 dk 15 sn | kırmızı |
| #64 (`a1e37ff`) | bilinmiyor | ? | ? | ? | 1 dk 37 sn | kırmızı |
| #65 (`bba155f`) | bilinmiyor | ? | ? | ? | 1 dk 36 sn | **yeşil** |
| #66 (`ddd4ba6`) | bilinmiyor | ? | ? | ? | 1 dk 58 sn | **yeşil** |

Toplam süreler Actions listesinden alındı. Job bazlı süreler (`?`) için ilgili koşuya tıklayıp her job'ın süresini yazmak yeterli. `gh` girişi varsa: `gh run list --workflow CI --limit 5` ve `gh run view <id> --json jobs`.

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
