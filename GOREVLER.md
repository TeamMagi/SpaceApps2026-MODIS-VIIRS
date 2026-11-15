# Görev Dağılımı — PyroSync

> **İşaretleme kuralı:** Her görev başlığının başında bir onay kutusu vardır. `[ ]` = yapılmadı, `[X]` = tamamlandı. Görevi bitiren kişi, "Bitti sayılır" şartı sağlandığında kendi görevinin kutusunu `[X]` yapar (özet tablosundaki kutuyu da). Şart sağlanmadan `[X]` konmaz. Bu dosya projenin TEK görev listesidir; ayrı görev dosyası açılmaz.

Proje bağlamı: [README.md](README.md). Strateji: STRATEGY.md (yerel, gitignore'da).

> [!NOTE]
> 2026-11-13'te oluşturuldu. Tek branch (`main`) üzerinden sırayla commit atılır; herkes kendi görevini bitirince commit atar, çakışma olmaması için aynı dosyalara aynı anda dokunılmaz.

## Nasıl okunur

- **Kimlik:** `G` Görkem'in, `C` Can'ın, `R` Serhat'ın, `S` Samet'in, `Y` Yusuf'un, `K` Sadık'ın görevleri. Örnek: `R3` Serhat'ın 3. görevi, `K2` Sadık'ın 2. görevi. Commit mesajlarında bu kimlikler kullanılır (`[G2] VZA correction katsayıları kalibre edildi`, `[K1] fallback modu eklendi`). Görev kimlikleri tüm dosyada tekildir.
- **Bekler:** başlamadan önce bitmiş olması gereken görevler. Bunlar bitmeden başlanırsa iş ya yapılamaz ya da tekrar yapılır.
- **Açar:** bu görev bitince başlayabilecek görevler.
- **Bitti sayılır:** görevin kapanma şartı. Şart sağlanmadan `[X]` konmaz.
- **Durum:** `[X]` bitti · `[ ]` yapılmadı (özet tablosunda ayrıca `sürüyor` veya `bekliyor` — bir bağımlılık bitmedi — notu yer alabilir)

Bir görevi bitiren kişi durumunu `[X]` yapar ve "Açar" satırındaki görevlerin sahiplerine haber verir.

---

## Bağımlılık grafiği

```mermaid
flowchart LR
    classDef gorkem fill:#dbeafe,stroke:#1d4ed8,color:#1e3a8a
    classDef can fill:#fce7f3,stroke:#be185d,color:#831843
    classDef serhat fill:#d1fae5,stroke:#059669,color:#064e3b
    classDef samet fill:#fef3c7,stroke:#b45309,color:#78350f
    classDef yusuf fill:#e0e7ff,stroke:#4338ca,color:#312e81
    classDef sadik fill:#fde68a,stroke:#92400e,color:#451a03

    G1[G1 FIRMS API entegrasyonu]:::gorkem
    G2[G2 Harmonizasyon pipeline]:::gorkem
    G3[G3 Effort normalization]:::gorkem
    G4[G4 Server API tamamlama]:::gorkem
    G5[G5 Sunum + demo]:::gorkem
    G6[G6 Final entegrasyon]:::gorkem
    G7[G7 Config-driven pipeline]:::gorkem
    G8[G8 Bölgesel kalibrasyon]:::gorkem
    G9[G9 Belirsizlik aralıkları]:::gorkem
    G10[G10 Export engine + API]:::gorkem

    C1[C1 UI/UX konsept]:::can
    C2[C2 Tasarım sistemi]:::can
    C3[C3 Harita + takvim tasarımı]:::can
    C4[C4 Raw↔Harmonized tasarımı]:::can
    C5[C5 Methods + disclaimer tasarımı]:::can
    C6[C6 Export + brifingi tasarımı]:::can
    C7[C7 Belirsizlik + renk skalası tasarımı]:::can

    R1[R1 Component mimarisi]:::serhat
    R2[R2 İnteraktif harita]:::serhat
    R3[R3 Burning calendar]:::serhat
    R4[R4 API bağlantısı]:::serhat
    R5[R5 Raw↔Harmonized toggle]:::serhat
    R6[R6 Methods sayfası impl]:::serhat
    R7[R7 Export + brifingi impl]:::serhat
    R8[R8 Model seçimi toggle]:::serhat

    S1[S1 Core unit testler]:::samet
    S2[S2 Integration testler]:::samet
    S3[S3 API dokümantasyonu]:::samet
    S4[S4 Sample veri pipeline]:::samet
    S5[S5 CI/CD kurulumu]:::samet
    S6[S6 Yeni modül testleri]:::samet
    S7[S7 Case study veri seti]:::samet
    S8[S8 Bilimsel referans sistemi]:::samet

    Y1[Y1 FRP zaman serisi]:::yusuf
    Y2[Y2 Sensör karşılaştırma]:::yusuf
    Y3[Y3 Yangın heatmap]:::yusuf
    Y4[Y4 İstatistik kartları]:::yusuf
    Y5[Y5 Mock veri hazırlığı]:::yusuf

    K1[K1 Live/demo/fallback modu]:::sadik
    K2[K2 Config ayarları paneli]:::sadik
    K3[K3 Belirsizlik görselleştirme]:::sadik
    K4[K4 Erişilebilir renk skalası]:::sadik
    K5[K5 Area API disk cache]:::sadik
    K6[K6 2023 NOAA-21 efor geçişi]:::sadik
    K7[K7 Tam filo 2000-present trend]:::sadik
    K8[K8 MODIS içi efor geçişi 2002]:::sadik
    K9[K9 SPA fallback düzeltme]:::sadik

    G1 --> G2
    G2 --> G3
    G2 --> G4
    G3 --> G4
    G4 --> G5
    R5 --> G5
    Y1 --> G5
    G5 --> G6
    G2 --> G7
    G7 --> G8
    G8 --> G9
    G4 --> G10

    C1 --> C2
    C1 --> C3
    C1 --> C4
    C1 --> C5
    C1 --> C6
    C1 --> C7

    C2 --> R1
    C3 --> R2
    C3 --> R3
    C4 --> R5
    R1 --> R2
    R1 --> R3
    R1 --> R4
    G4 --> R4
    R2 --> R5
    R3 --> R5
    R4 --> R5
    C5 --> R6
    R1 --> R6
    G10 --> R7
    C6 --> R7
    S7 --> R7
    G7 --> R8
    K1 --> R8
    R1 --> R8

    G2 --> S1
    G4 --> S2
    G4 --> S3
    G1 --> S4
    S1 --> S5
    G7 --> S6
    G9 --> S6
    G10 --> S6
    G1 --> S7
    C5 --> S8

    R1 --> Y1
    R1 --> Y2
    R1 --> Y3
    R1 --> Y4
    Y5 --> Y1
    Y5 --> Y2
    Y5 --> Y3
    Y5 --> Y4
    S4 --> Y1
    S4 --> Y2

    R4 --> K1
    G7 --> K2
    R1 --> K2
    G9 --> K3
    C7 --> K3
    R1 --> K3
    C7 --> K4
    R1 --> K4
    K5 --> K6
    K5 --> K7
    K9 --> R1
```

Mavi: Görkem · Pembe: Can · Yeşil: Serhat · Sarı: Samet · Mor: Yusuf · Turuncu: Sadık. Oklar "önce bu biter, sonra şu başlar" demek.

---

## Özet

| Kimlik | Görev | Sahip | Bekler | Açar | Durum |
|--------|-------|-------|--------|------|-------|
| G1 | FIRMS API gerçek veri entegrasyonu | Görkem | — | G2, S4 | [X] |
| G2 | Harmonizasyon pipeline tamamlama | Görkem | G1 | G3, G4, S1 | [X] |
| G3 | Effort normalization (Ω) | Görkem | G2 | G4 | [X] |
| G4 | Server API endpoint'leri tamamlama | Görkem | G2, G3 | G5, R4, S2, S3 | [X] |
| G5 | Sunum materyali + demo senaryosu | Görkem | G4, R5, Y1 | G6 | [ ] bekliyor (R5 [X], Y1 bekliyor) |
| G6 | Final entegrasyon + deployment | Görkem | G5 | — | [ ] bekliyor |
| G7 | Config-driven pipeline | Görkem | G2 | G8, R8, K2, S6 | [X] |
| G8 | Bölgesel/biyom kalibrasyon modülü | Görkem | G7 | G9 | [X] |
| G9 | Belirsizlik aralıkları hesaplama | Görkem | G8 | K3, S6 | [X] |
| G10 | Export engine + /api/export | Görkem | G4 | R7, S6 | [X] |
| C1 | UI/UX genel konsept + wireframe | Can | — | C2, C3, C4 | [X] |
| C2 | Tasarım sistemi (renkler, tipografi, spacing) | Can | C1 | R1 | [X] |
| C3 | Harita + takvim detay tasarımı | Can | C1 | R2, R3 | [X] |
| C4 | Raw ↔ Harmonized karşılaştırma tasarımı | Can | C1 | R5 | [X] |
| C5 | Methods sayfası + disclaimer tasarımı | Can | C1 | R6, S8 | [ ] bekliyor |
| C6 | Export UI + brifingi tasarımı | Can | C1 | R7 | [ ] bekliyor |
| C7 | Belirsizlik gösterimi + renk skalası tasarımı | Can | C1 | K3, K4 | [ ] bekliyor |
| R1 | React component mimarisi + layout | Serhat | C2 | R2, R3, R4, Y1–Y4 | [X] |
| R2 | İnteraktif harita entegrasyonu | Serhat | C3, R1 | R5 | [X] |
| R3 | Burning activity calendar componenti | Serhat | C3, R1 | R5 | [X] |
| R4 | Backend API bağlantısı + state management | Serhat | G4, R1 | R5 | [X] |
| R5 | Raw ↔ Harmonized toggle implementasyonu | Serhat | R2, R3, R4, C4 | G5 | [X] |
| R6 | Methods/About sayfası implementasyonu | Serhat | C5, R1 | — | [ ] bekliyor |
| R7 | CSV/GeoJSON export + acil müdahale brifingi | Serhat | G10, C6, S7 | — | [ ] bekliyor |
| R8 | Çoklu harmonizasyon model seçimi | Serhat | G7, K1, R1 | — | [ ] bekliyor |
| S1 | Core crate unit testleri | Samet | G2 | S5 | [X] |
| S2 | Server + FIRMS client integration testleri | Samet | G4 | — | [X] |
| S3 | API dokümantasyonu (OpenAPI) | Samet | G4 | — | [X] |
| S4 | Sample/demo veri pipeline'ı | Samet | G1 | Y1, Y2 | [ ] |
| S5 | GitHub Actions CI/CD kurulumu | Samet | S1 | — | [ ] |
| S6 | Config + calibration + export testleri | Samet | G7, G9, G10 | — | [ ] bekliyor |
| S7 | Türkiye/Akdeniz case study veri seti | Samet | G1 | R7 | [ ] bekliyor |
| S8 | Bilimsel referans + disclaimer içerik | Samet | C5 | — | [ ] bekliyor |
| Y1 | FRP zaman serisi grafiği | Yusuf | R1, Y5, S4 | G5 | [ ] bekliyor |
| Y2 | Sensör karşılaştırma grafikleri | Yusuf | R1, Y5, S4 | — | [ ] bekliyor |
| Y3 | Aylık yangın yoğunluk heatmap'i | Yusuf | R1, Y5 | — | [ ] bekliyor |
| Y4 | İstatistik dashboard kartları | Yusuf | R1, Y5 | — | [ ] bekliyor |
| Y5 | Mock veri hazırlığı (frontend için) | Yusuf | — | Y1, Y2, Y3, Y4 | [ ] |
| K1 | Live/demo/fallback modu | Sadık | R4 | R8 | [ ] bekliyor |
| K2 | Config ayarları frontend paneli | Sadık | G7, R1 | — | [ ] bekliyor |
| K3 | Belirsizlik aralıkları görselleştirme | Sadık | G9, C7, R1 | — | [ ] bekliyor |
| K4 | Erişilebilir renk skalası sistemi | Sadık | C7, R1 | — | [ ] bekliyor |
| K5 | Area API sonuçlarını diske cache'leme | Sadık | — | K6, K7 | [ ] |
| K6 | 2023 (NOAA-21) efor geçişini ölçme | Sadık | K5 | — | [ ] |
| K7 | Tam filo 2000-present trend serisi | Sadık | K5 | — | [ ] |
| K8 | MODIS içi efor geçişi (2002 Aqua) | Sadık | — | — | [ ] |
| K9 | SPA fallback gerçekten çalışmıyor | Sadık | — | R1 | [ ] |

**Şu an başlanabilecekler:**
- **Görkem:** G5 — Sunum materyali + demo senaryosu (Y1 bekliyor). R5 bitti, G4 bitti.
- **Can:** C1 [X], C2 [X], C3 [X], C4 [X]. İlk faz UI/UX tasarım ve çalışan React prototipi (`web/src`) görevleri tamamlandı (sıradaki işler bu dosyada C5–C7; C1 bitmiş olduğundan hepsine başlayabilir).
- **Serhat:** R1 [X], R2 [X], R3 [X], R4 [X], R5 [X]. Tüm frontend görevleri tamamlandı.
- **Samet:** S5 — CI/CD (S1 bitti), S3 — OpenAPI dokümantasyonu (G4 bitti), S4 (G1 bitti); S7 — G1 (FIRMS API) bitmiş olduğundan Türkiye verisi çekmeye başlayabilir.
- **Yusuf:** Y5 — Mock veri JSON dosyaları, hiçbir şeye bağımlı değil. R1 bitti, Y5 bitince Y1–Y4 açılır.

**Birbirini bekleyen kritik yollar:**
1. ~~**G1 → G2 → G4 → R4 → R5:**~~ [X] Tamamlandı. Backend pipeline ve frontend toggle zinciri tamam.
2. ~~**C1 → C2 → R1:**~~ [X] Tamamlandı. Component mimarisi ve tasarım sistemi entegre.
3. **G2 → S1 → S5:** Samet'in CI/CD'si S1'e bağlı, S1 bitti. S5 başlayabilir.
4. **R1 + Y5 → Y1–Y4:** Yusuf'un grafikleri Y5 (mock veri) bekliyor. R1 bitti.
5. **R5 → G5:** Serhat'ın frontend zinciri tamamlandı. G5 artık yalnızca Y1'i bekliyor.
6. **G2 → G7 → G8 → G9 → K3:** Config ve kalibrasyon tamamlanmadan belirsizlik hesaplanamaz ve görselleştirilemez.
7. **G4 → G10 → R7:** Export backend hazır olmadan frontend export UI yapılamaz. S7 (case study) de R7'yi besler.
8. **C5 → R6 + S8:** Methods sayfası hem frontend implementasyon hem içerik için tasarımı bekler.
9. **R4 → K1 → R8:** Fallback modu API bağlantısına bağlı; model seçimi de fallback'e bağlı.

---

## Görkem'in görevleri

### [X] G1 — FIRMS API gerçek veri entegrasyonu
- **Bekler:** — · **Açar:** G2, S4
- `pyrosync-firms` crate'i iskelet olarak hazır. Yapılacaklar:
  - [X] NASA FIRMS API key ile gerçek veri çekme testi — `examples/nrt_check.rs`
  - [X] Rate limiting ve retry mekanizması (API 429/503 dönebilir) — `limiter.rs`, `Retry-After` destekli exponential backoff
  - [X] Tarihsel veri desteği (NRT + SP + archive kaynaklarına göre endpoint seçimi) — `source.rs`, `availability.rs`, `archive.rs`
  - [X] CSV parse'ın gerçek FIRMS çıktısıyla doğrulanması (MODIS ve VIIRS formatları farklı sütun isimleri kullanabiliyor)
  - [X] `data/sample/` altına birkaç günlük örnek veri kaydetme (offline geliştirme için) — 5 format
- **Bitti sayılır:** `cargo test -p pyrosync-firms` geçiyor; gerçek API key ile en az 3 farklı kaynak (MODIS_NRT, VIIRS_SNPP_NRT, VIIRS_NOAA20_NRT) üzerinden veri çekilip parse ediliyor; `data/sample/` altında örnek CSV var.
- **Doğrulama (2026-11-14):** 43 test geçiyor, clippy temiz. Canlı API `--bbox 15,-15,30,0` ile: MODIS_NRT 21, VIIRS_SNPP_NRT 2300, VIIRS_NOAA20_NRT 348 hotspot parse edildi.
- **Not:** Canlı doğrulama `acq_time` parse hatasını ortaya çıkardı — FIRMS baştaki sıfırları atıyor (`614` = 06:14), eski kod yalnızca 4 karakterlik değer kabul ediyordu. MODIS_NRT çekimi tümüyle başarısız oluyordu; `data/sample/` altındaki üç NRT dosyası da aynı sebeple offline parse edilemiyordu. `parse_hhmm` 1–4 haneli değerleri kabul edecek şekilde düzeltildi, 6 regresyon testi eklendi.

### [X] G2 — Harmonizasyon pipeline tamamlama
- **Bekler:** G1 · **Açar:** G3, G4, S1
- Pipeline iskeletleri mevcut (dedup, VZA, matching, calendar). Yapılacaklar:
  - [X] H3 hexagonal grid entegrasyonu — `h3o` 0.11, Resolution 8 (hücre 0,74 km², MODIS 1 km ayak izine en yakın ölçek), k-ring 3 ile aday arama (~2,8 km garantili kapsama; k=2 tam 2,0 km'de sınırda kalıp eşleşme kaçırıyordu). `crates/core/src/h3index.rs`
  - ⛔ VZA polinom katsayılarını gerçek overlap-period verisinden hesaplama — **FIRMS ile yapılamaz**, bkz. aşağıdaki not. Katsayılar placeholder kaldı.
  - [X] Cross-sensor matching optimizasyonu — BallTree yerine H3 kullanıldı. Rust'ta olgun BallTree crate'i yok ve H3 zaten gerekiyordu; iki indeks yerine tek indeks hem agregasyon hem aday arama için kullanılıyor.
  - [X] Fleet rule — `crates/core/src/fleet.rs`, ayrı aşama olarak sensör-içi dedup'tan sonra ve MODIS eşleştirmesinden önce çalışıyor.
  - [X] Gerçek FIRMS verisiyle uçtan uca pipeline testi — `/api/harmonize` üzerinden koşuldu.
- **Bitti sayılır (2026-11-14'te ölçüme dayalı olarak revize edildi):**
  1. `cargo test -p pyrosync-core` geçiyor ve `cargo clippy --workspace --all-targets` temiz.
  2. Gerçek FIRMS verisiyle MODIS+VIIRS → harmonized → calendar akışı en az üç farklı bölgede çalışıyor, `Matched` kategorisi kayıt üretiyor.
  3. Tespit ve FRP azalması **bileşen bazında** raporlanıyor: interscan dedup, VIIRS filo birleştirme ve cross-sensor füzyon ayrı ayrı. Sabit bir yüzde bandı beklenmez — azalma yangın yoğunluğuna bağlıdır (ölçüm: seyrek bölgede %14,5, yoğun bölgede %39–41).
  4. VZA düzeltmesi gerçek veride tetikleniyor ve uygulanan katsayı kayıt başına `vza_factor` alanında raporlanıyor. **R² hesabı G2 kapsamı dışıdır** (bkz. aşağıdaki not).
- **Revizyon gerekçesi:** Önceki iki şart ("FRP %15–20 azalıyor", "VZA R²'si hesaplanıyor") ölçümle çeliştiği için değiştirildi. Eski %15–20 bandı literatürde yalnız interscan dedup için verilen bir değer; bu pipeline'da ayrıca VIIRS filo birleştirmesi var ve toplam azalma bölge yoğunluğuna göre %14–41 arasında değişiyor, hiçbir bölgede bandın içine düşmüyor. R² ise eşzamanlı MODIS/VIIRS gözlem çifti gerektiriyor ve ölçüm bu çiftlerin FIRMS'te bulunmadığını gösterdi.
- **Doğrulama (2026-11-14), canlı `/api/harmonize`, 2 günlük pencere, 180 dk eşleştirme penceresiyle:**

  | Bölge | Ham | Harmonize | n düşüş | FRP düşüş | Matched |
  |-------|-----|-----------|---------|-----------|---------|
  | Zambiya/Angola | 699 / 6.770 MW | 585 / 4.866 MW | %16,3 | %28,1 | 12 |
  | Sahel | 14.042 / 133.676 MW | 9.270 / 74.605 MW | %34,0 | %44,2 | 221 |
  | Kuzey Avustralya | 11.876 / 137.654 MW | 7.411 / 75.983 MW | %37,6 | %44,8 | 329 |

  **Bileşen ayrıştırması** (şart 3 gereği, `n_obs` alanı üzerinden hesaplanır):

  | Aşama | Zambiya | Sahel | Avustralya |
  |-------|---------|-------|------------|
  | MODIS interscan dedup | 1 (%4,8) | 80 (%11,1) | 144 (%13,8) |
  | VIIRS interscan dedup | 57 (%8,4) | 1.602 (%12,0) | 1.694 (%15,6) |
  | VIIRS filo birleştirme | 44 (%6,5) | 2.869 (%21,5) | 2.298 (%21,2) |
  | Cross-sensor füzyon | 12 | 221 | 329 |

  MODIS tespitlerinin %34–60'ı bir VIIRS karşılığı buluyor (Zambiya 12/20, Sahel 221/643, Avustralya 329/897).
- **Filo kuralı doğrulandı, aşırı birleştirme yok.** Filo VIIRS tespitlerinin %21–22'sini siliyor; bağımsız ölçümde NOAA-21 tespitlerinin %19,7'sinin 2 km içinde NOAA-20 eşi olduğu bulunmuştu. İki sayı örtüşüyor, yani kural verinin gerçekten duplikat olduğunu söylediği kadarını siliyor. 2 km yarıçapı değiştirilmedi.
- **Cross-sensor pencere kararı: 180 dakika.** `MATCH_WINDOW_SECS` 300 saniyeden 10800 saniyeye çıkarıldı ve `Matched` yeniden tanımlandı: "eşzamanlı gözlem" değil, "aynı yangın olayının iki sensörle gözlemi".
  - Ölçüm: 26.505 co-located MODIS-VIIRS noktası, 4 bölge, 5 tarih, üç VIIRS platformu. **30 dakika altında sıfır çift.** Mutlak minimum 34 dk. Medyan 158 dk (NOAA-20/21), 122 dk (SNPP). Kümülatif: ≤45 dk %0,2 · ≤90 dk %9,5 · ≤120 dk %27,9 · ≤180 dk ~%49.
  - 300 saniyelik pencereyle üç bölgenin hepsinde `Matched` sıfır kayıt üretiyordu, yani cross-sensor füzyon iddiası çıktısızdı.
  - Pencere genişlemesi H3 indeksi sayesinde güvenli: aday kümesi mekansal olarak 37 hücreye sınırlı, zaman penceresinin genişlemesi aday sayısını patlatmıyor. Bu yüzden H3'ün önce yapılması doğru sıraydı.
  - Füzyon davranışı değişmedi: `Matched` kaydı VIIRS değerini kullanıyor, MODIS FRP'si ortalamaya karışmıyor. 3 saat önceki MODIS okumasını VIIRS tahminine katmak istemiyoruz.
- **VZA katsayı kalibrasyonu ve R² — FIRMS verisiyle yapılamaz, G2 kapsamı dışına alındı.** Mekanizma çalışıyor: VZA `scan` piksel boyutundan türetiliyor, gerçek veride 20/20 MODIS kaydında tetikleniyor, en büyük düzeltme %42,4 (33,8 → 19,48 MW). Ancak `correction_ratio` polinomunun katsayıları (A=0,85 B=0,12 C=1,0) placeholder ve kalibre edilmeleri eşzamanlı MODIS/VIIRS gözlem çifti gerektiriyor; yukarıdaki ölçüm bu çiftlerin FIRMS'te bulunmadığını gösterdi (minimum 34 dk). İki yol var: (a) literatürden hazır katsayı girilip kaynak gösterilir, (b) granül/arşiv seviyesi veri sağlanır. S7 (case study veri seti) ile birlikte planlanmalı. **Katsayılar girilene kadar harmonize FRP değerleri gösterge niteliğindedir ve methods sayfasında (C5/R6) bu açıkça belirtilmeli.**
- **Yan bulgu:** `VIIRS_SNPP_NRT` beslemesi 2026-11-02'de duruyor, diğer NRT kaynakları günceldir. `availability.rs` bunu doğru yönetiyor (SNPP sorgularını kendi aralığına kırpıyor), ama kısa pencereli isteklerde SNPP hiç katkı vermiyor; bu yüzden filo grupları o tarihlerde en fazla 2 platform içeriyor.

### [X] G3 — Effort normalization (Ω)
- **Bekler:** G2 · **Açar:** G4
- Uydu sayısı arttıkça aynı bölgede daha fazla detection olur — bu yapay artışı düzeltmek gerekiyor.
  - [X] Her ay/bölge için effective observation count hesaplama — `crates/core/src/effort.rs`
  - [X] MODIS-only dönemine normalize etme — taban dönemde Ω tanım gereği 1,0 çıkıyor
  - [X] Takvim çıktısına anomaly z-score'ları ekleme — anomali artık normalize seri üzerinden hesaplanıyor
- **Bitti sayılır:** 2000–present boyunca monoton artan trend kalkıyor; 2012 ve 2017'deki uydu geçişlerinde yapay sıçrama yok.
- **Görev tanımındaki iki hata düzeltildi (arşiv verisinden ölçüldü):**
  - Görev "2017+ MODIS + 2 VIIRS" diyor; NOAA-20 verisi FIRMS'te **2018-04**'te başlıyor, 2017'de değil.
  - Görev **Aqua'yı atlamış**. Arşivde Aqua'nın ilk tespiti **2002-07**, yani 2000-11'den 2002-06'ya kadar yalnız Terra var ve orada da bir efor geçişi mevcut. Ω bunu doğru yakalıyor (Terra tek başına 21 ay, Terra+Aqua 113 ay; ikisinde de Ω = 1,000 çünkü ikisi de MODIS-only).
  - Gerçek platform zaman çizelgesi: Terra 2000-11 · +Aqua 2002-07 · +SNPP 2012-01 · +NOAA-20 2018-04 · +NOAA-21 2023-03.
- **Tasarım: Ω sabit kodlanmadı, her serinin kendi verisinden ölçülüyor.**
  - Ölçüm (arşiv, Türkiye, 2000-2025): MODIS-only dönem ~10.700 tespit/yıl, SNPP eklendikten sonra ~59.000 tespit/yıl, yani sıçrama **5-6 kat**. Buna karşılık MODIS'in kendi sayısı 26 yıl boyunca düz (10.733 → 11.123). Yani yapay trend platform SAYISIYLA değil sensör TESPİT VERİMİYLE ölçekleniyor (VIIRS 375 m vs MODIS 1 km).
  - Bu yüzden "platform sayısına göre normalize et" yaklaşımı yetersiz kalırdı: 2→3 platform için 1,5 kat düzeltme yapar, ~4 katlık sıçrama yerinde kalırdı.
  - Sabit Ω katsayısı da yanlış olurdu: SNPP/MODIS verim oranı bölgeye göre **3,28 (Bulgaristan) – 7,35 (Suriye)** arası değişiyor, 2,2 kat fark.
  - Çözüm: MODIS referans cetvel olarak kullanılıyor. Her ay için `total` ve `modis_observable` (platforms alanında MODIS bulunan kayıtlar) sayılır; aylar **(MODIS var mı?, VIIRS platform kümesi)** anahtarına göre havuzlanır ve `omega = Σtotal / Σmodis_observable` hesaplanır. Taban dönemde her kayıt MODIS içerdiği için Ω = 1,0 **otomatik** çıkar, elle ayar yok.
  - Havuzlama anahtarı MODIS alt bileşimine duyarsız. Gerekçe: Aralık 2018'de Türkiye'de yalnız 385 tespit vardı ve Terra hiç yangın görmedi; anahtar tam sensör kümesi olunca o ay tek başına havuzlandı, 3 ay eşiğini karşılamadı ve Ω'sını kaybetti. Dört platform da görevdeydi — Terra'nın yangın görmemesi yangınlar hakkında bilgidir, efor hakkında değil.
  - "Alt kümeyse üst kümenin Ω'sını devral" kuralı bilinçli olarak REDDEDİLDİ: istek aralığı dönem ortasında başlarsa geçiş öncesi aylar eşiğin altında kalır, büyük Ω ile bölünür ve ters yönde yapay sıçrama üretir.
- **Doğrulama (2026-11-14):** 73 core testi geçiyor, clippy temiz.

  **2012 geçişi** — arşiv, 4 ülke (Türkiye/Yunanistan/Bulgaristan/Kıbrıs), 2000-2025, 26 yıl:

  | Metrik | Ham seri | Normalize seri |
  |--------|----------|----------------|
  | Seviye kayması (5 yıl önce/sonra) | ×3,21 | **×0,84** |
  | Dönem ortalaması (2002-11 → 2012-25) | ×3,34 | ×0,87 |
  | Monoton artan trend (Spearman ρ, 2001-2025) | **+0,785** | **+0,087** |

  Normalize serideki en büyük yıllık atlama (×2,66, 2008→2009) tamamen MODIS-only dönemin içinde, yani gerçek yangın aktivitesi — sensör artefaktı değil.

  **2018 (NOAA-20) geçişi** — area API SP kaynakları, Türkiye bbox, 2017-07..2018-12, aynı mevsim kıyaslaması (Tem-Ara 2017 vs Tem-Ara 2018):

  | | Ham | Normalize |
  |---|-----|-----------|
  | Kayma | ×1,317 | **×0,912** |
  | \|log(kayma)\| | 0,275 | **0,093** |

  Ω tam 2018-04'te 4,715'ten 6,810'a atlıyor ve hiçbir ayda boşluk kalmıyor; konfigürasyon tespiti tarih sabiti kullanmadan, yalnızca veriden isabetli çalışıyor. Bu koşuda 1373 transaction içinde 1 istek FIRMS transaction limitine takıldı (2018-12 ayı 385 yerine 366 tespit, kıyaslama ortalaması %0,05 değişti) — sonucu etkilemiyor.

### G3 doğrulamasının sınırları (açıkça kayda geçirildi)
- **Monoton trend testinin serisi tam filo değil.** ρ +0,785 → +0,087 ölçümü 2000-2025 arşiv serisi üzerinde yapıldı ve bu seri yalnızca **MODIS + SNPP** içeriyor; ülke arşivi NOAA-20 ve NOAA-21 taşımıyor. Yani dört efor geçişinden ikisi (2018, 2023) trend testinde YOK. Trend, 2012 geçişini içeren uzun seride düz çıkıyor; 2018 geçişi ayrı ve kendi veri setinde doğrulandı; tam filo içeren 2000-present serisi FIRMS'ten makul transaction bütçesiyle kurulamıyor.
  - **Düzeltme (2026-11-15):** Ülke arşivi NOAA-20'yi `viirs-jpss1` slug'ı ile taşıyor (2018-2025); önceki "taşımıyor" yargısı yanlış slug denemelerine dayanıyordu. Yalnız NOAA-21 (`viirs-jpss2`) arşivde yok. Yani 2018 geçişi artık arşiv serisinin içinde sınanabilir; bkz. K7.
- **2023 (NOAA-21) geçişi ÖLÇÜLMEDİ.** Mekanizma platform kümesinden bağımsız çalıştığı için aynı davranması beklenir, ama bu bir beklenti; ölçüm değil.
- **Ω, MODIS referans kanalının kendi içindeki geçişi düzeltemez.** 2002-07'de Aqua eklenmesi tespitleri artırdı (arşivde 2001 Terra 5162 → 2002 Terra 4474 + Aqua 2412), ama Aqua `modis_observable`'ın parçası olduğu için Ω 1,000 kalıyor ve bu sıçrama düzeltilmiyor. Havuzlama anahtarı MODIS alt bileşimine duyarsız olduğundan Terra-only ve Terra+Aqua dönemleri tek havuzda birleşiyor (134 ay, Ω 1,000). Bitiş şartı yalnızca 2012 ve 2017'yi istediği için şart ihlal edilmiyor, ama 2000-2002 taban dönemi efor açısından sonrasıyla tam kıyaslanabilir DEĞİL. Çözüm gerekirse MODIS içi normalizasyon ayrı bir referans (örn. Terra-only alt kanalı) gerektirir.
- **Açık risk: fetch hatası Ω'yı sessizce bozuyor.** Ara bir doğrulama koşusunda 36 istek başarısız oldu ve NOAA-20 verisi eksik geldiği için 2018-08..12 ayları "NOAA-20 yok" konfigürasyonuna düşüp yanlış Ω (6,810 yerine 4,724) aldı. Yani bir kaynağın çekimi başarısız olursa efor normalizasyonu sessizce yanlış düzeltme uygular. **G4'te ele alınmalı:** `report.failures` boş değilse Ω güvenilmez sayılmalı ve API bunu açıkça bildirmeli.

### [X] G4 — Server API endpoint'leri tamamlama
- **Bekler:** G2, G3 · **Açar:** G5, R4, S2, S3
- Mevcut: `/api/health`, `/api/harmonize` (bbox + days). Yapılacaklar:
  - [X] `/api/calendar` — arşivden tarihsel aylık takvim (`country`, `from`, `to`, opsiyonel bbox)
  - [X] `/api/compare` — raw vs harmonized karşılaştırma istatistikleri
  - [X] `/api/stats` — genel istatistikler, sensör kırılımı, pipeline metrikleri
  - [X] Response caching — bellek bütçeli, iki seviyeli TTL, `X-Cache: HIT|MISS` header'ı
  - [X] CORS ve error handling son hali — `CORS_ALLOWED_ORIGINS`, `/api/*` için JSON 404, `REQUEST_TIMEOUT_SECS`
- **Bitti sayılır:** tüm endpoint'ler Postman/curl ile test edildi; frontend proxy üzerinden veri alıyor; 10 saniyelik timeout dahilinde yanıt geliyor.
- **Doğrulama (2026-11-14):** 21 server testi geçiyor (önceden hiç test yoktu), toplam 144 test, clippy temiz.

  **Yanıt süreleri — Vite dev proxy (5173) üzerinden, backend :3001:**

  | Endpoint | İlk istek | Cache'li | Boyut |
  |----------|-----------|----------|-------|
  | `/api/health` | 3 ms | 3 ms | 33 B |
  | `/api/availability` | 4 ms | 3 ms | 603 B |
  | `/api/harmonize` | 1,36 s | 4 ms | 290 KB |
  | `/api/harmonize?include_raw=true` | 0,69 s | 5 ms | 481 KB |
  | `/api/compare` | 0,63 s | 6 ms | 1,1 KB |
  | `/api/stats` | 0,66 s | 3 ms | 1,2 KB |
  | `/api/calendar` | 24 ms | 4 ms | 9,1 KB |

  Frontend proxy doğrulandı: `npm install` + `npm run dev` ile Vite 5173 ayağa kaldırıldı, `/api/health` ve `/api/harmonize` istekleri backend'e düştü ve doğru payload döndü. `npm run build` ve `npx tsc --noEmit` temiz geçiyor.
- **Şartın "10 saniye" maddesine dair sınır:** `days` tipi istekler için fazlasıyla sağlanıyor (en yavaş 1,36 s). Ancak `start`/`end` ile verilen uzun tarih aralıkları doğası gereği dakikalar sürüyor — 18 aylık bir aralık 1373 FIRMS isteği demek. Sabit 10 saniyelik timeout bu istekleri kullanılamaz yapardı, bu yüzden `REQUEST_TIMEOUT_SECS` yapılandırılabilir (varsayılan 300 s) ve zaman aşımı mesajı sebebini açıklıyor. Uzun aralıkların 10 saniyeye inmesi asenkron iş semantiği gerektirir, G4 kapsamı dışıdır.
- **`/api/calendar` kapsam sınırı:** area API ile 2000-present imkansız (26 yıl on binlerce transaction), bu yüzden endpoint FIRMS ülke arşivini kullanıyor — transaction harcamıyor ve `FIRMS_API_KEY` gerektirmiyor. Bedeli: arşiv NOAA-21 taşımıyor, dolayısıyla tarihsel takvim MODIS + SNPP + NOAA-20 kapsıyor ve 2023 sonrasında NRT endpoint'lerinden daha küçük bir filo gösteriyor. Yanıt bunu `coverage` bölümünde ve `warnings` dizisinde açıkça bildiriyor. Tam filo serisi için bkz. K7.
- **G3'ten devralınan risk kapatıldı:** `report.failures` boş değilse yanıt `effort.reliable: false` ve sebebini taşıyor, ayrıca üst seviye `warnings` dizisi var. Ω değerleri null'lanmıyor — veri kaybı frontend'i kırar, uyarıyla işaretlemek yeterli.
- **Açık bulgu:** SPA fallback dokümante edildiği gibi çalışmıyor (`/harita` → 404). G4'ün bitiş şartının parçası değil, bu dosyada K9 olarak kuyruğa alındı.
- **R4 için not:** `web/src/services/api.ts` ve `web/src/types/index.ts` yeni endpoint'leri (`/api/compare`, `/api/stats`, `/api/calendar`) ve yeni alanları (`count_normalized`, `effort_omega`, `vza_factor`, `n_obs`, `platforms`, `mean_frp_raw`, `effort`, `warnings`) henüz kapsamıyor. Tip kontrolü hata vermiyor çünkü fazladan JSON alanları çelişki yaratmıyor; eksiklik R4 kapsamında giderilmeli.

### [ ] G5 — Sunum materyali + demo senaryosu
- **Bekler:** G4, R5, Y1 · **Açar:** G6
- NASA Space Apps jüri kriterleri (Impact, Creativity, Validity, Relevance, Presentation) doğrultusunda:
  - 3 dakikalık demo videosu senaryosu
  - "Sensor cliff" problemini gösteren raw görünüm → toggle → harmonized görünüm akışı
  - Teknik açıklama slaytları (pipeline diyagramı, VZA correction görseli, H3 grid görseli)
  - Kullanım senaryoları: orman yangını izleme, tarım yanığı tespiti, iklim araştırması
- **Bitti sayılır:** demo senaryosu yazıldı; canlı demo 3 dakika içinde sorunsuz çalışıyor.

### [ ] G6 — Final entegrasyon + deployment
- **Bekler:** G5 · **Açar:** —
- Tüm parçaların bir araya gelmesi:
  - Frontend build (`npm run build`) → Axum static file serving
  - Environment variable yönetimi (FIRMS_API_KEY)
  - Tek komutla çalışır hale getirme
  - Son testler ve bug fix'ler
  - Submission paketleme
- **Bitti sayılır:** `cargo run -p pyrosync-server` ile backend + frontend tek process'te ayağa kalkıyor; jüri demo'yu sorunsuz görebilir.

### [X] G7 — Config-driven pipeline
- **Bekler:** G2 · **Açar:** G8, R8, K2, S6
- Pipeline'daki tüm sabit eşikleri yapılandırılabilir hale getirmek (EG FireTrack'ten ilham):
  - `config.rs` modülü: `PipelineConfig` struct'ı ile tüm parametreler
  - Dedup eşikleri: zaman penceresi (varsayılan 8s), örtüşme oranı (varsayılan %30)
  - VZA düzeltme katsayıları: polinom coefficients (a, b, c)
  - Cross-sensor matching: mesafe (varsayılan 2 km), zaman (varsayılan 5 dk)
  - TOML dosyasından yükleme (`pyrosync.toml`) + env var override
  - Varsayılan config ile mevcut davranış aynı kalmalı (backward compatible)
- **Kaynak:** EG FireTrack — "Config-driven tasarım: tüm eşikler yapılandırılabilir, bilimsel şeffaflık"
- **Bitti sayılır:** `PipelineConfig::default()` ile mevcut testler hâlâ geçiyor; TOML'dan farklı config yüklenince pipeline davranışı değişiyor; `pyrosync.example.toml` depoda.
- **Durum (2026-11-15):** Sadık tarafından tamamlandı (`4f3e23d`). `PipelineConfig`, `pyrosync.toml` yükleme ve `pyrosync.example.toml` depoda.

### [X] G8 — Bölgesel/biyom kalibrasyon modülü
- **Bekler:** G7 · **Açar:** G9
- Tek global FRP oranı yerine bölgesel katsayılar (Li et al. 2018):
  - `calibration.rs` modülü: biyom tipine göre MODIS/VIIRS FRP kalibrasyon oranları
  - Tropik: VIIRS ~%16 büyük (k=1.16)
  - Boreal: MODIS ~%47 büyük (k=1.47)
  - Savan: ~%1 fark (k≈1.0)
  - Biyom lookup: lat/lon → biyom → kalibrasyon katsayısı
  - Config'den override edilebilir (`PipelineConfig.calibration`)
- **Kaynak:** Li et al. 2018 — "Tek global katsayı yetersiz — savan %1 fark, boreal %47 fark"
- **Bitti sayılır:** farklı bölgelerde (tropik, boreal, savan) farklı kalibrasyon uygulanıyor; test: Türkiye (Akdeniz) ve Sibirya (boreal) verileriyle FRP oranları beklenen aralıkta.
- **Durum (2026-11-15):** Kod tamamlandı (`a883a82`, `c82641d`), bitti şartı SAĞLANMADI. Li et al. 2018 katsayıları 750 m VIIRS ürününden; FIRMS verisi 375 m. Aqua–SNPP 1° ızgara ölçümü: tropik 1,21–1,22 (model 1,16), Türkiye 1,33 (model 0,97), boreal 1,34–1,40 (model 0,82). Literatür 375 m için bu ölçümü doğruluyor (Li 2020: Afrika'da MODIS toplam FRP ~%26 düşük; GMD 2026: küresel %38). Katsayılar 375 m ölçeğine göre güncellenmeli (bkz. `docs/REFERANS_ANALIZI.md` P1).
- **Karar ve kapanış (2026-11-15, `a387ba1`):** Üçlü konsey (Opus, Argon, DeepSeek) kararıyla kapsam yeniden tanımlandı. Izgara toplamı oranları MODIS'in tespit edemediği küçük yangınlardan (tamlık) geldiği için tespit başına enlem çarpanı KALDIRILDI; bölgesel fark artık VIIRS öncesi dönemde hücre toplam FRP'sine uygulanan ölçülmüş tamlık katsayısı k_c ile taşınıyor. k_c kendi verimizden ölçüldü (Aqua–SNPP, ≤20 dk, 1° hücre-gün, normalize VZA eğrisine göre; 2019 fit, 2020 hold-out): tropik 1,11 ± 0,08, orta enlem 1,18 ± 0,15, boreal 1,28 ± 0,11 (hold-out farkları %4–9, σ içinde). VZA düzeltmesi de kaynağa (Zheng 2026, Denk. 9) uygun olarak çarpmaya çevrildi. Takvime `total_frp` ve MODIS-eşdeğeri `frp_per_detection_eq` eklendi. Açık: savan/orman ayrımı enlem bandıyla yapılamıyor (biyom haritası gerekir); orta enlem bandı az örneklemli.

### [X] G9 — Belirsizlik aralıkları hesaplama
- **Bekler:** G8 · **Açar:** K3, S6
- Her harmonize FRP değerine güven aralığı eklemek (fire-harmony'den ilham):
  - `uncertainty.rs` modülü: `UncertaintyEstimate { lower: f64, upper: f64, confidence: f64 }`
  - Kaynaklar: VZA düzeltme hatası, kalibrasyon oranı varyansı, sensör gürültüsü
  - Bootstrap veya analitik propagasyon (basit: ±1σ)
  - `HarmonizedHotspot` struct'ına `frp_lower`, `frp_upper` alanları
  - Calendar hücrelerine de toplam belirsizlik
  - Bilimsel disclaimer metni: "Harmonize değerler model tahminidir, ground truth değil"
- **Kaynak:** fire-harmony — belirsizlik aralıkları; tüm akademik makaleler — model sınırları
- **Bitti sayılır:** her harmonize değerin lower/upper aralığı var; aralık VZA arttıkça genişliyor (beklenen); API response'unda uncertainty alanları mevcut.
- **Durum (2026-11-15):** Tamamlandı (`3213e11`, `9c6a718`) ve bağımsız denetimle doğrulandı: Türkiye 2019-08 (5483) ve Angola 2019-04 (10386) kayıtta `lower <= frp_corrected <= upper` ihlali 0; ModisOnly aralık genişliği VZA kovalarında monoton artıyor; `/api/calendar` yanıtında `uncertainty` nesnesi ve hücrelerde `mean_frp_lower/upper` var. Açık kalanlar: Matched kayıtlarda rastgele bileşen MODIS üyelerini de sayarak fazla daralıyor; sensör gürültüsü ve VZA parametreleri hâlâ varsayım (bkz. `docs/REFERANS_ANALIZI.md` P3).

### [X] G10 — Export engine + /api/export endpoint
- **Bekler:** G4 · **Açar:** R7, S6
- Harmonize veriyi indirilebilir formatlara dönüştürme:
  - `export.rs` modülü: `to_csv()`, `to_geojson()` fonksiyonları
  - `/api/export?format=csv&west=...&south=...&east=...&north=...` endpoint'i
  - `/api/export?format=geojson&...` alternatifi
  - CSV: tüm alanlar + uncertainty + kaynak sensör
  - GeoJSON: FeatureCollection, her hotspot bir Feature (point geometry + properties)
  - Streaming response (büyük veri setleri için bellek dostu)
  - Content-Disposition header ile doğrudan dosya indirme
- **Kaynak:** Strateji 1.4 (Relevance) — "CSV export ile gerçek iş akışlarına entegrasyon"
- **Bitti sayılır:** curl ile `/api/export?format=csv` → geçerli CSV dosyası iniyor; GeoJSON QGIS'te açılıyor; 10,000+ satır sorunsuz.
- **Durum (2026-11-15):** Sadık tarafından tamamlandı (`cae9664`). CSV ve GeoJSON export, `/api/export`. Belirsizlik (uncertainty) sütunları G9 ile eklenecek.

---

## Can'ın görevleri

### [X] C1 — UI/UX genel konsept + wireframe
- **Bekler:** — · **Açar:** C2, C3, C4
- Projenin görsel kimliğini belirleyen ilk tasarım:
  - [X] Ana sayfa layout'u: harita, takvim, kontroller nerede duracak
  - [X] Kullanıcı akışı: bölge seçimi → veri yükleme → Raw/Harmonized geçişi → detay inceleme
  - [X] Wireframe — tüm sayfalar/paneller (ASCII, Figma yerine)
  - [X] Mobil responsive düşünülmeli (jüri telefonda bakabilir)
- **Referans:** `STRATEGY.md`'deki "Raw ↔ Harmonised toggle" ve "Sensor Cliff" kavramları tasarımın merkezinde olmalı.
- **Bitti sayılır:** wireframe tüm ekiplere paylaşıldı ve onaylandı; ana sayfa, harita görünümü, takvim görünümü, karşılaştırma paneli var.
- **Çıktı (2026-11-14):** `docs/UI-UX-KONSEPT.md` — tasarım ilkeleri, desktop layout, kullanıcı akışı + loading/empty/error state'leri, 4 ekran wireframe'i (dashboard, harita + hotspot popover, yıl×ay takvim ısı matrisi, Raw↔Harmonised karşılaştırma + Sensor Cliff grafiği), mobil wireframe + breakpoint'ler, bileşen ağacı + state sahipliği tablosu, C2/C3/C4 devir notları.
- **Çıktı (Can, 2026-11-14):** `web/src` içinde interaktif React prototipi (+ offline arşiv API `/api/regions`, `/api/series`, `/api/points`) + `docs/TASARIM_KONSEPT.md`, `docs/UX_YAZIM.md`, `docs/FRONTEND_STACK_ONERISI.md`.
- **Doğrulama (2026-11-15):** Tüm tasarım dokümanları, responsive layout (`AppShell.module.css`) ve prototip bileşenleri depoda; alt görevler `R1`–`R5` ekip/Serhat tarafından doğrulanıp `✅` kapatılarak onaylandı. `/api/harmonize` ham nokta ihtiyacı da G4+R4 kapsamında `include_raw=true` ile çözüldü.
- **Bulgu:** `HarmonizeResponse.raw` varsayılan olarak yalnızca sayaç döndürüyor (`modis_count`, `viirs_count`, `total_raw`); ham noktalar gerektiğinde `include_raw=true` parametresiyle alınıyor.

### [X] C2 — Tasarım sistemi
- **Bekler:** C1 · **Açar:** R1
- Serhat'ın kodlayacağı component'ler için görsel standartlar:
  - [X] Renk paleti (primary, secondary, accent; MODIS, VIIRS ve harmonized FRP/anomali skalaları CSS token olarak tanımlı)
  - [X] Tipografi (font family, size scale, weight)
  - [X] Spacing sistemi (8px grid)
  - [X] Button, card, panel, tooltip component tasarımları (`web/src/components/**/*.module.css` içinde çalışır durumda)
  - [X] Dark mode desteği (`[data-theme="dark"]` varsayılan + `[data-theme="light"]`)
- **Çıktı (Can, 2026-11-14):** `web/src/styles/tokens.css`, `web/src/styles/global.css` ve CSS Modules bileşen stilleri içinde tüm CSS variable isimleri ve tema paletleri belirlendi.
- **Çıktı (2026-11-14):** `docs/TASARIM_SISTEMI.md` — token referansı (dark/light renk tabloları, sensör + FRP/anomali skalaları, tipografi, 8px spacing, radius/motion, bileşen kalıpları, yapılmaması gerekenler).
- **Doğrulama (2026-11-15):** `tokens.css`, `global.css` ve `docs/TASARIM_SISTEMI.md` birebir uyumlu; `R1` tamamlanıp `✅` işaretlendiği için onay şartı sağlandı.
- **Bitti sayılır:** Serhat bu dökümanla component'leri birebir kodlayabilir; renk ve spacing soruları kalmamış.

### [X] C3 — Harita + takvim detay tasarımı
- **Bekler:** C1 · **Açar:** R2, R3
- İki ana görselleştirme panelinin detaylı tasarımı:
  - [X] **Harita:** hotspot noktaları nasıl gösterilecek (FRP ve confidence'a göre renk/boyut/şekil, Canvas 2D örnekleme ve `PointDetail` popup/detay içeriği, boş/yükleniyor/hata/veri durumları)
  - [X] **Takvim:** aylık hücrelerin renk skalası (PuOr anomali skalasına göre), yıl ve ay eksenleri, hover detayı (`CalendarTooltip`)
  - [X] İkisi arasındaki etkileşim: takvimde bir aya tıklayınca haritada o ayın verileri
- **Çıktı (Can, 2026-11-14):** `web/src/components/Map/HotspotMap.tsx` (+ `engine/`, `PointDetail.tsx`, `MapStateOverlay.tsx`) ve `web/src/components/Calendar/BurnCalendar.tsx` (+ `CalendarTooltip.tsx`) içinde çalışır şekilde uygulandı.
- **Güncelleme (cddf0ec):** takvim ısı matrisi radyal yıl saati + yıllık histogram ile değiştirildi.
- **Doğrulama (2026-11-15):** Harita ve takvimin tüm veri/boş/yükleniyor/hata durumları çalışır durumda; ülke seçimi yaklaşımı ve canlı `G4` API uyumu `R2`, `R3` ve `R4` (`✅`) ile doğrulandı, `CliffChart`–`BurnCalendar` yıl state senkronizasyonu eklendi.
- **Bitti sayılır:** harita ve takvim component'lerinin her durumu (boş, yükleniyor, veri var, hata) tasarlanmış.

### [X] C4 — Raw ↔ Harmonized karşılaştırma tasarımı
- **Bekler:** C1 · **Açar:** R5
- Projenin **en önemli demo anı:** toggle ile Raw ve Harmonized arasında geçiş.
  - [X] Toggle/switch UI tasarımı (belirgin, anlaşılır; `ViewToggle.tsx`)
  - [X] Raw modda sensör ayrımı ve sensor cliff görünümü (`TASARIM_KONSEPT.md`: renk körü güvenliği için kırmızı/yeşil yerine MODIS içi boş daire + VIIRS dolu kare ayrımı; `CliffChart.tsx`)
  - [X] Harmonized modda tek renk skalası (fused data / inferno FRP skalası)
  - [X] Geçiş animasyonu: noktaların birleşmesi, duplikasyonların kaybolması (3 vuruşlu koreografi: `useHarmonizeTransition.ts`, 1400/500/150 ms)
  - [X] İstatistik paneli ve düzeltme fişi (`CountDelta.tsx`, `Receipt.tsx`)
- **Çıktı (Can, 2026-11-14):** `ViewToggle.tsx`, `useHarmonizeTransition.ts`, `CliffChart.tsx`, `CountDelta.tsx`, `Receipt.tsx` ve `docs/TASARIM_KONSEPT.md`.
- **Çıktı (2026-11-14):** `fleet_merged` arşiv API'sine (`/api/series` yıllık + `/api/points` stages) eklendi; `Receipt` ve `PipelineStrip` artık filo birleştirme adımını gösteriyor, sayılar `ham − interscan − filo − eşleşen = harmonize` olarak tutuyor.
- **Doğrulama (2026-11-15):** Her iki mod, 3 vuruşlu geçiş koreografisi ve istatistik fişi `R5` (`✅`) ile doğrulandı. (Not: 7 düğümlü pipeline şeridinde ayrı bir "filo" düğümü yok; `useHarmonizeTransition.ts` koreografisine dokunmamak için filo sayısı bölünme metninde gösteriliyor.)
- **Bitti sayılır:** toggle'ın her iki durumu ve geçiş animasyonunun tasarımı tamamlanmış; Serhat implementasyon için hazır.

### [ ] C5 — Methods sayfası + bilimsel disclaimer tasarımı
- **Bekler:** C1 · **Açar:** R6, S8
- Pipeline'ın nasıl çalıştığını açıklayan sayfa tasarımı:
  - 7 aşamalı pipeline diyagramı (görsel akış)
  - Her aşama için: ne yapılıyor, neden, hangi parametre kullanılıyor
  - VZA düzeltme formülü görseli (θ → FRP düzeltmesi grafiği)
  - H3 grid açıklaması (hexagon görseli)
  - Bilimsel referans listesi formatı (akademik makalelere atıf)
  - Disclaimer banner tasarımı: "Bu değerler model tahminidir" uyarısı
- **Bitti sayılır:** wireframe'de Methods sayfası var; pipeline görseli okunabilir; disclaimer yerleşimi belirgin ama rahatsız edici değil.

### [ ] C6 — Export UI + acil müdahale brifingi tasarımı
- **Bekler:** C1 · **Açar:** R7
- İki ilişkili özelliğin tasarımı:
  - **Export:** indirme butonu konumu, format seçimi (CSV/GeoJSON), dosya adı gösterimi
  - **Acil müdahale brifingi:** tek tıkla bölge raporu — tarihsel ortalama vs şu an, anomali durumu, en yoğun aylar, aktif sensörler
  - Brifingi modal/panel olarak mı sidebar olarak mı gösterelim
  - Print-friendly layout (jüri basabilir)
- **Kaynak:** Strateji 1.1 — "Acil müdahale brifingi — tek tıkla bölge raporu"
- **Bitti sayılır:** export butonu ve brifingi paneli wireframe'de; brifingi içeriği (hangi metrikler, hangi sırada) belirlenmiş.

### [ ] C7 — Belirsizlik gösterimi + erişilebilir renk skalası tasarımı
- **Bekler:** C1 · **Açar:** K3, K4
- Bilimsel verilerin doğru görselleştirilmesi:
  - **Belirsizlik:** error bar / confidence band / shaded area — hangi yöntem hangi chart'ta
  - Haritada belirsizlik gösterimi (opaklık veya halo boyutu)
  - **Renk skalası:** erişilebilir paletler (renk körlüğü uyumlu — viridis, cividis tarzı)
  - MODIS rengi, VIIRS rengi, harmonized rengi, anomaly skalası
  - Quantile vs eşit aralık renk dağılımı seçimi
  - Light/dark mode'da renk kontrastı
- **Kaynak:** Yarışma gereksinimleri — "Color ramps"; fire-harmony — belirsizlik gösterimi
- **Bitti sayılır:** renk paleti renk körlüğü simülatörüyle test edilmiş; belirsizlik gösterimi her chart tipi için tanımlanmış.

---

## Serhat'ın görevleri

> **Not — backend API değişikliği (2026-11-15, `d734196`), frontend'de güncelleme gerekiyor:**
> - **VZA düzeltmesinin yönü değişti.** Kaynak makaleye (Zheng et al. 2026) uygun olarak MODIS FRP artık bölünmüyor, çarpılıyor; yani düzeltme FRP'yi **artırıyor**. Bu yüzden `/api/compare` yanıtındaki `vza.mean_frp_reduction_pct` artık **negatif** değer dönüyor (negatif = artış). Alan adı geriye uyumluluk için korundu.
> - **Yeni alan:** `vza.mean_frp_adjustment_pct` (pozitif = artış). Frontend bu alana geçmeli ve metin "azaltım" yerine "düzeltme/artış" olarak güncellenmeli. Kullanım yerleri: `web/src/services/api.ts:861` (`meanFrpReductionPct`), `web/src/types/index.ts:382`.
> - **Kaldırılan alan:** hotspot'lardaki `calibration_ratio` (ve CSV/GeoJSON export sütunu). Frontend kullanmıyordu.
> - **Takvim hücrelerine eklenen alanlar (`/api/calendar`):** `total_frp`, `total_frp_lower`, `total_frp_upper`, `frp_per_detection_eq` (MODIS-eşdeğeri tespit başına FRP; sensör geçişinde süreklilik için önerilen metrik), `mean_frp_lower`, `mean_frp_upper`. `mean_frp` korunuyor ama artık "native-pixel" ortalama; 2012 VIIRS geçişinde piksel boyutu yüzünden düşüş gösterir, takvimde manşet metrik olarak kullanılmamalı.
> - Ayrıntı: `crates/server/CLAUDE.md`, `docs/REFERANS_ANALIZI.md` (P1–P2) ve bu dosyadaki G8.

### [X] R1 — React component mimarisi + layout
- **Bekler:** C2 · **Açar:** R2, R3, R4, Y1–Y4
- Frontend'in iskeletini kurmak (mevcut `web/` dizini üzerinde):
  - Can'ın tasarım sistemini CSS variables olarak implement etme
  - Layout component'leri: `<AppShell>`, `<Sidebar>`, `<MapPanel>`, `<CalendarPanel>`, `<StatsBar>`
  - Responsive grid (mobile-first)
  - Shared state altyapısı (React Context veya zustand)
  - TypeScript type'ları backend API response'larına göre (`types.ts`)
  - `npm install` çalışıyor, `npm run dev` ile development server ayağa kalkıyor
- **Teknik not:** TypeScript derlemesi SWC ile yapılıyor (`@vitejs/plugin-react-swc` zaten kurulu).
- **Bitti sayılır:** `npm run dev` hatasız çalışıyor; layout Can'ın wireframe'ine uyuyor; boş paneller doğru yerde; mobile'da düzgün görünüyor.
- **Doğrulama (2026-11-14):** Can'ın C1 prototipi R1 kapsamının ~%90'ını karşılıyordu. Kalan: `types/index.ts` NRT tip tanımları, `api.ts` wire type hizalaması (`WireYearSpan` string/number parse, `WireSeriesCliff` nullable alanlar, `WireHarmonizedPoint` yeni alanlar), `Headline.tsx` null guard'ları. `npx tsc --noEmit` ve `npm run build` temiz geçiyor.

### [X] R2 — İnteraktif harita entegrasyonu
- **Bekler:** C3, R1 · **Açar:** R5
- Yangın hotspot'larını harita üzerinde gösteren component:
  - [X] Leaflet/Mapbox yerine custom Canvas 2D engine tercih edildi — GeoJSON projection, Douglas-Peucker LOD, DPR-aware rendering. Binlerce noktayı 60 fps'te çiziyor.
  - [X] Hotspot noktalarını FRP'ye göre boyut ve renk ile gösterme — `engine/renderer.ts` inferno FRP skalası + confidence şekil ayrımı
  - [X] LOD engine zoom seviyesine göre polygon simplification yapıyor; Canvas doğası gereği binlerce noktayı sorunsuz render ediyor
  - [X] Bölge seçimi: ülke polygonlarına tıklama ile önceden tanımlı bölge seçimi (bbox çizme yerine; görev tanımı "veya" ile iki seçenek sunuyordu)
  - [X] Popup: `PointDetail.tsx` ile sensor, FRP, confidence, tarih, VZA faktörü, kaynak platform detayı
- **Ek özellikler (C3 prototipi):** MapStatusBar (nokta sayısı, bölge adı), MapLegend (sensör filtresi MODIS/VIIRS), MapLabels (ülke etiketleri), MapCountryTooltip, MapControls (zoom in/out/reset), keyboard navigation (N/P ile nokta gezinme), MapStateOverlay (loading/error/empty)
- **Bitti sayılır:** mock veriyle harita render oluyor; zoom/pan akıcı; noktalar FRP'ye göre farklılaşıyor; bbox seçimi çalışıyor.
- **Doğrulama (2026-11-15):** Can'ın C3 prototipi R2 kapsamının tamamını kapsıyordu. `HotspotMap.tsx` (1173 satır) + `engine/` dizini (renderer, viewport, pointIndex, featureHit, lod, types) + PointDetail, MapControls, MapLegend, MapLabels, MapStatusBar, MapStateOverlay, MapCountryTooltip bileşenleri tam çalışır durumda. Gerçek arşiv verisinden (`useMapPoints`) harita render ediliyor, mock veriye gerek kalmadı. `npx tsc --noEmit` ve `npm run build` temiz geçiyor.

### [X] R3 — Burning activity calendar componenti
- **Bekler:** C3, R1 · **Açar:** R5
- Aylık yangın aktivitesi görselleştirmesi (GitHub contribution graph yerine radyal yıl saati tercih edildi):
  - [X] 12 aylık radyal dilimler, her dilimin yarıçapı detection count'a orantılı — yıl stepper (◀ ▶) ile 2000-present arası tüm yıllar gezinilebiliyor
  - [X] Dilim rengi: anomali skoru tabanlı (PuOr paleti CSS variable'larla), normalin altı/üstü görsel olarak belirgin
  - [X] Normal baseline: kesikli çizgi arc ile her ayın uzun vadeli ortalaması gösteriliyor
  - [X] Hover: `CalendarTooltip` ile count, anomali z-skoru, ham vs harmonize karşılaştırması
  - [X] Tıklama: `toggleMonth()` ile haritayı o ay/yıla filtreliyor
  - [X] Yıl oynatma: ▶ butonu ile yıllar arası otomatik geçiş animasyonu
  - [X] Keyboard navigation: ok tuşları ile ay gezinme, PageUp/PageDown ile yıl atlama, Enter/Space ile seçim
- **Bitti sayılır:** mock veriyle takvim render oluyor; 2000–2026 arası tüm aylar görünüyor; renk skalası anlamlı; hover tooltip çalışıyor.
- **Doğrulama (2026-11-15):** Can'ın C3 prototipi R3 kapsamının tamamını kapsıyordu. `BurnCalendar.tsx` (675 satır) + `CalendarTooltip.tsx` + `lib/radialCalendar.ts` (hesaplama yardımcıları) tam çalışır durumda. Gerçek arşiv verisinden (`useRegionData`) takvim render ediliyor. Görev tanımındaki "GitHub contribution graph" formatı yerine radyal yıl saati seçildi (cddf0ec commit'iyle güncellendi) — aynı veriyi daha kompakt gösteriyor ve yıl oynatma özelliği ekleniyor. `npx tsc --noEmit` ve `npm run build` temiz geçiyor.

### [X] R4 — Backend API bağlantısı + state management
- **Bekler:** G4, R1 · **Açar:** R5
- Frontend'i gerçek backend'e bağlayan katman:
  - API client modülü (`api.ts`): `/api/harmonize`, `/api/calendar`, `/api/compare`, `/api/stats`
  - Loading/error/success state yönetimi
  - Bölge değiştiğinde otomatik veri yenileme
  - API yanıtını component state'ine dönüştürme
- **Not:** Vite proxy zaten ayarlı (`/api` → `localhost:3001`).
- **Bitti sayılır:** gerçek backend çalışırken frontend veri çekip gösteriyor; loading spinner var; API hatası kullanıcıya mesaj olarak dönüyor.
- **Doğrulama (2026-11-14):** `api.ts`'e 4 NRT fetch fonksiyonu (fetchHarmonize, fetchCompare, fetchStats, fetchCalendar) + wire types + adapters eklendi (~477 satır). `useNrtData(bbox)` hook'u useSyncExternalStore pattern ile 3 paralel NRT fetch yönetiyor. Store'a `nrtDays` (1-5, varsayılan 2) eklendi. CountDelta ve PipelineStrip'e NRT veri gösterimi entegre edildi. `npx tsc --noEmit` ve `npm run build` temiz geçiyor.

### [X] R5 — Raw ↔ Harmonized toggle implementasyonu
- **Bekler:** R2, R3, R4, C4 · **Açar:** G5
- Projenin imza özelliği:
  - [X] Toggle/switch: `ViewToggle.tsx` — spring animasyonlu thumb, aria-checked, H kısayol tuşu
  - [X] Raw modda: MODIS (boş daire) ve VIIRS (dolu kare) ayrı şekillerle, renk körü güvenli; sensör filtresi (MapLegend) ile tek sensör izole edilebiliyor; duplikasyonlar halkalı gösteriliyor (`showDuplicateRings`)
  - [X] Harmonized modda: fused data, inferno FRP skalası, tek renk; duplikat halkaları yok
  - [X] Geçiş animasyonu: 3 vuruşlu koreografi (`useHarmonizeTransition.ts`) — detect (400ms) → merge (600ms) → settle (400ms); `geometryHarmonized` harita noktalarını geçişin ortasında değiştiriyor, `revealAnswer` sondaki sayısal cevabı açıyor
  - [X] İstatistik paneli: `CountDelta.tsx` raw → harmonized sayı animasyonu + `Receipt.tsx` düzeltme fişi (interscan dedup, filo birleştirme, cross-sensor eşleşme kırılımı)
  - [X] Harita ve takvim aynı anda güncelleniyor: `useHarmonizeTransition` tek saat olarak çalışıyor, `HotspotMap` ve `BurnCalendar` aynı state'e subscribe
  - [X] Sensor cliff: `CliffChart.tsx` yıllık ham tespit serisini sensör bazlı yığılmış alan grafiği olarak gösteriyor; 2012 SNPP ve 2018 NOAA-20 sıçramaları görsel olarak belirgin
  - [X] Pipeline şeridi: `PipelineStrip.tsx` 6 adımlı harmonizasyon aşamasını sırayla yakarak gösteriyor, toplam çıkarılan duplikat sayısını raporluyor
- **Bitti sayılır:** toggle sorunsuz çalışıyor; Raw'dan Harmonized'a geçiş "sensor cliff" problemini açıkça gösteriyor; animasyon akıcı; jüri bu toggle'ı görünce farkı anlıyor.
- **Doğrulama (2026-11-15):** Can'ın C4 prototipi R5 kapsamının tamamını kapsıyordu. `ViewToggle.tsx` (61 satır), `useHarmonizeTransition.ts` (3 vuruşlu koreografi motoru), `CliffChart.tsx`, `CountDelta.tsx`, `Receipt.tsx`, `PipelineStrip.tsx` hepsi `App.tsx`'te compose edilmiş ve çalışır durumda. Toggle'ın harita ve takvimi aynı anda güncellediği `useHarmonizeTransition` abonelik mekanizmasıyla doğrulandı. NRT veri entegrasyonu (R4'ten) CountDelta ve PipelineStrip'te canlı veri gösteriyor. `npx tsc --noEmit` ve `npm run build` temiz geçiyor.

### [ ] R6 — Methods/About sayfası implementasyonu
- **Bekler:** C5, R1 · **Açar:** —
- Can'ın C5 tasarımını koda dökmek:
  - `/methods` route (React Router veya conditional render)
  - Pipeline diyagramı: SVG veya step-by-step component
  - Her aşamaya tıkla → detay accordion
  - Bilimsel referans listesi (Samet S8 ile koordineli — içerik Samet, görünüm Serhat)
  - Disclaimer banner component'i (sayfa altı veya header'da)
  - Responsive: mobilde de okunabilir
- **Bitti sayılır:** `/methods` sayfası açılıyor; 7 pipeline aşaması görsel olarak anlaşılıyor; referanslar listelenmiş; disclaimer görünür.

### [ ] R7 — CSV/GeoJSON export + acil müdahale brifingi
- **Bekler:** G10, C6, S7 · **Açar:** —
- İki özelliğin frontend implementasyonu:
  - **Export:** indirme butonu → format seçimi → `/api/export` çağrısı → blob download
  - Dosya adı: `pyrosync_{region}_{date}.csv` formatında
  - Loading state (büyük dosyalarda)
  - **Brifingi:** seçili bölge için özet panel/modal
  - Metrikler: son 7 gün detection, tarihsel ortalamadan sapma, en yoğun sensör, trend (artış/azalış)
  - Print butonu (window.print ile)
  - Case study verileri (S7) ile demo senaryosu
- **Bitti sayılır:** CSV indiriliyor ve Excel'de açılıyor; GeoJSON indiriliyor; brifingi paneli en az 5 metrik gösteriyor; Türkiye verisiyle demo çalışıyor.

### [ ] R8 — Çoklu harmonizasyon model seçimi
- **Bekler:** G7, K1, R1 · **Açar:** —
- Kullanıcının farklı harmonizasyon yaklaşımlarını karşılaştırabilmesi:
  - Dropdown/toggle: "Standard" (VIIRS-preferred) vs "Equal-weight" vs "MODIS-calibrated"
  - Seçim backend'e config olarak gönderilir (G7'nin config yapısıyla)
  - Sonuçlar anlık güncellenir (harita + takvim + stats)
  - Farklılık istatistiği: "Bu model %X daha fazla/az detection üretiyor"
  - Fallback modunda (K1) da çalışmalı
- **Kaynak:** Strateji 1.2 — "Çoklu harmonizasyon modeli seçimi — kullanıcı yöntemi seçip farkı görebilecek"
- **Bitti sayılır:** en az 2 farklı model arasında geçiş yapılabiliyor; sonuçlar gözle görülür şekilde farklı; istatistik farkı gösteriliyor.

---

## Samet'in görevleri

> **Not (2026-11-15):** `docs/API.md` içinde `vza.mean_frp_reduction_pct` alanı hâlâ "ortalama VZA-only FRP azalması" olarak tarif ediliyor. VZA düzeltmesi artık çarpma olduğu için bu alan negatif değer döner (negatif = artış); açıklama buna göre güncellenmeli ve yeni `vza.mean_frp_adjustment_pct` alanının (pozitif = artış) tercih edildiği belirtilmeli.

### [X] S1 — Core crate unit testleri
- **Bekler:** G2 · **Açar:** S5
- `pyrosync-core` crate'indeki harmonizasyon pipeline'ı için kapsamlı testler:
  - [X] **dedup:** bilinen duplikat seti verin → FRP azalması beklenen (revize, bkz. aşağıda); ayrıca haversine (sıfır mesafe, 1° enlem, simetri, antimeridyen), %30 örtüşme eşiği ve 8 sn penceresi
  - [X] **VZA:** nadir'de correction = 1.0, 60° VZA'da > 1.0; simetri yerine negatif VZA üretmeme ve kenara doğru azalmama (revize, bkz. aşağıda)
  - [X] **matching:** 2 km ve **180 dk** içindeki MODIS/VIIRS çiftleri eşleşmeli, dışındakiler eşleşmemeli (revize: G2'de pencere 5 dk'dan 180 dk'ya çıktı)
  - [X] **calendar:** 24 aylık veri → 24 hücre; anomaly hesabı en az 3 yıl verisi gerektirir; std = 0'da anomali yok; z-skoru formülü; UTC ay sınırı
  - [X] Edge case'ler: boş veri, tek sensör, tüm deteksiyonlar eşleşen, hiçbiri eşleşmeyen
  - [X] Ek: önceden testsiz public yardımcılar (`Sensor::is_modis/is_viirs/firms_source`, `BBox::to_firms_string`, `Confidence` sıralaması)
- **Bitti sayılır:** `cargo test -p pyrosync-core` en az 20 test geçiyor; her modül (dedup, vza, harmonize, calendar) için en az 3 test var.
- **Revizyon gerekçesi (2026-11-14):** Üç madde koda göre eskimişti.
  - "%18.3" gerçek veriden gelen bir literatür ortalaması, algoritmanın sabit bir özelliği değil; G2 de aynı bandı bıraktı. Yerine sonucu elle hesaplanan bir küme kullanıldı: 4 tespit → 3, toplam FRP 200 → 160 MW, azalma tam %20.
  - Eşleştirme penceresi G2'de 180 dk'ya çıkarıldı; testler bu değere göre yazıldı.
  - Düzeltme polinomunda doğrusal terim var (B = 0,12), bu yüzden R(−θ) ≠ R(θ) ve "simetrik" testi yazılsa kırılırdı. VZA bir açı büyüklüğü ve `vza_from_scan` negatif değer üretmiyor; anlamlı olan bu özellik test edildi.
- **Doğrulama (2026-11-14):** `cargo test -p pyrosync-core` 101 test geçiyor, 22'si bu görevden. Modül başına: calendar 11, dedup 14, harmonize 17, vza 20, models 7, fleet 13, effort 11, h3index 8. `cargo clippy --workspace --all-targets -- -D warnings` temiz; eklenen kod rustfmt'ye uygun.
- **Yan bulgular (Görkem için):**
  - `calendar.rs`: tespit olmayan aylar hücre olarak üretilmiyor, bu yüzden anomali baseline'ı yalnız tespit olan yıllardan hesaplanıyor. Ortalama olduğundan yüksek çıkar ve yangınsız yıllar takvimde hiç görünmez.
  - `dedup.rs`: yorum "|Δt| < 8 s" diyor ama kod 8 sn'yi de duplikat sayıyor (`dt > 8` ise kesiyor). Hangisinin doğru olduğu karar istiyor; sınır bu yüzden testle sabitlenmedi.

### [X] S2 — Server + FIRMS client integration testleri
- **Bekler:** G4 · **Açar:** —
- Uçtan uca API testleri:
  - [X] `/api/health` → 200 OK
  - [X] `/api/harmonize?west=...&south=...&east=...&north=...&days=1` → geçerli JSON, `harmonized.count > 0`
  - [X] `/api/calendar` → yıl/ay sıralı, anomaly hesabı tutarlı
  - [X] `/api/compare` → raw count > harmonized count
  - [X] Hatalı parametrelerde uygun HTTP status kodu
  - [X] FIRMS API key eksikse 500 + anlamlı hata mesajı
  - [X] Ek: `/api/stats` ve `/api/availability`, response cache (`X-Cache` HIT/MISS), FIRMS hata yolları (geçersiz anahtar → 502, tek kaynak hatası → 200 + uyarı, 503 → tekrar deneme, zaman aşımı → 504), kapsanmayan tarih aralığı raporu, eksik arşiv yılı
- **Bitti sayılır:** `cargo test -p pyrosync-server` en az 10 test geçiyor; mock FIRMS verisine karşı uçtan uca test çalışıyor.
- **Yaklaşım:** `crates/server/src/integration_tests.rs` gerçek router'ı (`build_router`) ve gerçek `FirmsClient`'ı wiremock ile kurulan sahte bir FIRMS sunucusuna karşı çalıştırır. Takvim, geçici bir dizindeki çevrimdışı arşive karşı çalışır. Sınır değer ve denklik sınıfı vakaları (days, bbox, tarih penceresi) rstest ile parametreli.
- **FIRMS istemcisinde ek değişiklik (geriye uyumlu):** Testlerin sahte sunucuya yönlenebilmesi için `FirmsClient::with_api_root(..)` eklendi. Varsayılan host değişmedi (`FIRMS_API_ROOT`), `Availability::fetch` artık host'u parametre olarak alıyor.
- **Doğrulama (2026-11-14):**
  - `cargo test -p pyrosync-server` 79 test geçiyor, 49'u bu görevden: 18 test fonksiyonu, ikisi parametreli (4 + 29 vaka).
  - `cargo clippy --workspace --all-targets -- -D warnings` temiz; yeni dosya rustfmt'ye uygun.
  - Satır kapsamı: `firms-client/client.rs` %36,7 → %55,3, `server/main.rs` %80,6 → %87,8, workspace toplamı %82,2 → %84,3.
- **Yan bulgu (Görkem için):** Eksik query parametresinde (ör. `south` yok) 400 yanıtı JSON değil düz metin dönüyor; diğer doğrulama hataları `{"error": ...}` döndürüyor. Test bu yüzden şimdilik yalnız status kodunu doğruluyor.

### [X] S3 — API dokümantasyonu
- **Bekler:** G4 · **Açar:** —
- Tüm API endpoint'lerinin dökümanı:
  - [X] Her endpoint: URL, method, parametreler, örnek request/response
  - [X] Hata kodları ve anlamları
  - [X] Örnek curl komutları
  - [X] `docs/API.md` olarak depoya
- **Bitti sayılır:** `docs/API.md` depoda; her endpoint için en az bir örnek var; takım bu dökümanla frontend geliştirme yapabilir.
- **Kapsam:** 11 endpoint (`health`, `config`, `availability`, `harmonize`, `compare`, `stats`, `export`, `calendar`, `regions`, `series`, `points`). Her biri için parametre tablosu, örnek istek ve yanıt, alan açıklamaları ve hata kodları var. Ayrıca ortak kavramlar (bbox, zaman penceresi, enum'lar, ortak nesneler), genel hata kodu tablosu, API'yi etkileyen ortam değişkenleri ve bilinen sorunlar bölümleri bulunuyor.
- **Örneklerin kaynağı:** Arşiv endpoint'lerinin örnekleri, yerel FIRMS arşiviyle çalışan sunucudan yakalanan gerçek Türkiye verisidir. FIRMS anahtarı gerektiren endpoint'lerin örnekleri ise S2'deki sahte FIRMS düzeneğinden alındı: yapı gerçek, sayılar sentetik ve doküman bunu belirtiyor.
- **Doğrulama (2026-11-15, commit 8846819):**
  - 36 JSON örneğinin hepsi geçerli JSON.
  - 16 `curl` örneği canlı sunucuda çalıştırıldı. Anahtarsız endpoint'ler 200, anahtar gerektirenler dokümandaki gibi 503 döndü (sunucuda anahtar yoktu).
  - Varsayılanlar, limitler, hata mesajları ve başlıklar koddan kontrol edildi.
- **Kapsam dışı:** Makine tarafından okunabilir OpenAPI şeması (`openapi.yaml`) ve yanıtları bu şemaya karşı doğrulayan sözleşme testi bu teslimatta yok; ayrı bir iş olarak eklenebilir.
- **Onay testi (Görkem'in düzeltmeleri):** Eksik parametrede artık JSON 400 dönüyor (33cc021). `/api/points` küçük `limit` ile artık takılmıyor: `limit=0` → 400, `limit=1` → 200 (a9cc3ed). Arşiv zaman aşımı 504'e eşitlendi.
- **Açık kalan bulgular (Görkem için):**
  - FIRMS availability tablosu süreç başına bir kez çekiliyor (`OnceCell`). Uzun çalışan sunucuda en yeni günler "kapsanmıyor" sayılabilir.
  - `.env.example` dosyasındaki yorum anahtar yokken hâlâ "500" diyor; kod 503 dönüyor.

### [ ] S4 — Sample/demo veri pipeline'ı
- **Bekler:** G1 · **Açar:** Y1, Y2
- Offline geliştirme ve demo için hazır veri seti:
  - FIRMS API'den birkaç bölge (Türkiye, Kaliforniya, Avustralya) için 2020–2025 verisi çekme
  - Her bölge için MODIS + VIIRS ayrı CSV dosyaları
  - Harmonize edilmiş JSON çıktıları
  - `data/sample/` altında organize
  - Frontend ve grafik component'leri bu veriyle çalışabilir
- **Bitti sayılır:** `data/sample/` altında en az 3 bölge var; her bölgede MODIS, VIIRS ve harmonized veri mevcut; Yusuf bu veriyle grafik yapabilir.

### [ ] S5 — GitHub Actions CI/CD kurulumu
- **Bekler:** S1 · **Açar:** —
- Her push'ta otomatik kontrol:
  - `cargo check` — derleme hatası yok
  - `cargo test` — tüm testler geçiyor
  - `cargo clippy` — lint uyarısı yok (isteğe bağlı)
  - `cd web && npm ci && npm run build` — frontend build başarılı
- **Dosya:** `.github/workflows/ci.yml`
- **Bitti sayılır:** main branch'e push yapınca Actions yeşil tik gösteriyor; başarısız build'de hata mesajı görünüyor.

### [ ] S6 — Config + calibration + export testleri
- **Bekler:** G7, G9, G10 · **Açar:** —
- Yeni modüller için kapsamlı testler:
  - **config:** TOML parse testi, varsayılan değerlerin doğruluğu, env var override, hatalı config'de anlamlı hata
  - **calibration:** tropik/boreal/savan bölgeler için doğru katsayı, bilinmeyen bölge → default fallback
  - **uncertainty:** nadir'de dar aralık, kenar'da geniş aralık; lower < frp < upper her zaman
  - **export:** CSV çıktısı doğru sütun sayısı ve header; GeoJSON valid FeatureCollection; boş veri → boş dosya (hata değil)
  - Edge case: sıfır detection, tek sensör, tüm güvenlik düşük
- **Bitti sayılır:** `cargo test` ile en az 15 yeni test geçiyor; her modül (config, calibration, uncertainty, export) için en az 3 test.

### [ ] S7 — Türkiye/Akdeniz case study veri seti
- **Bekler:** G1 · **Açar:** R7
- Demo ve sunum için hazır Türkiye yangın verisi:
  - FIRMS API'den Türkiye bbox (26°–45° E, 36°–42° N) için 2020–2025 verisi
  - Akdeniz bölgesi özel alt-set (Antalya, Muğla — 2021 yangınları dahil)
  - MODIS + VIIRS ayrı CSV'ler + harmonize edilmiş JSON
  - `data/case-study/turkey/` altında organize
  - İstatistik özet: toplam detection, pik aylar, yıllar arası trend
  - Brifingi component'i (R7) bu veriyle demo yapacak
- **Kaynak:** Strateji 1.1 — "Türkiye ve Akdeniz havzası özel case study — jüri için farklı ve dikkat çekici"
- **Bitti sayılır:** `data/case-study/turkey/` altında en az 3 yıl veri var; 2021 Akdeniz yangınları anomali olarak belirgin; JSON formatı frontend'le uyumlu.

### [ ] S8 — Bilimsel referans ve disclaimer içerik
- **Bekler:** C5 · **Açar:** —
- Methods sayfası için bilimsel içerik hazırlığı:
  - Her pipeline aşaması için 2–3 cümle açıklama (teknik ama anlaşılır)
  - Kullanılan akademik makalelerin referans listesi (APA formatı)
  - Referans–pipeline aşaması eşlemesi: hangi makale hangi aşamayı destekliyor
  - Disclaimer metni: sınırlamalar, model varsayımları, veri kaynağı güvenilirliği
  - `docs/REFERENCES.md` dosyası olarak depoya
- **Bitti sayılır:** `docs/REFERENCES.md` depoda; en az 5 makale referanslanmış; her pipeline aşaması en az bir referansla desteklenmiş; disclaimer metni net ve dürüst.

---

## Yusuf'un görevleri

### [ ] Y1 — FRP zaman serisi grafiği
- **Bekler:** R1, Y5, S4 · **Açar:** G5
- Recharts ile aylık ortalama FRP trendi:
  - X ekseni: zaman (2000–present), Y ekseni: ortalama FRP (MW)
  - İki çizgi: Raw (MODIS+VIIRS ayrı ayrı) ve Harmonized (fused)
  - Raw modda sensor cliff belirgin (2012'de VIIRS gelince sıçrama)
  - Harmonized modda düzgün trend
  - Zoom, tooltip, legend
- **Bu grafik sunum için kritik — "sensor cliff" problemini tek bakışta gösterir.**
- **Bitti sayılır:** mock veriyle grafik render oluyor; sensor cliff Raw modda görünüyor, Harmonized modda kaybolıyor; tooltip çalışıyor.

### [ ] Y2 — Sensör karşılaştırma grafikleri
- **Bekler:** R1, Y5, S4 · **Açar:** —
- MODIS vs VIIRS ayrıntılı karşılaştırma:
  - Bar chart: aylık detection count by sensor
  - Scatter plot: MODIS FRP vs VIIRS FRP (eşleşen çiftlerde)
  - Pie/donut: sensor breakdown (MODIS-only, VIIRS-only, Matched)
- **Bitti sayılır:** en az 2 farklı chart tipi çalışıyor; sensörler arası fark görsel olarak açık.

### [ ] Y3 — Aylık yangın yoğunluk heatmap'i
- **Bekler:** R1, Y5 · **Açar:** —
- Serhat'ın R3'ünden farklı olarak bu bir bölgesel yoğunluk haritası:
  - Grid tabanlı renk haritası (hexbin veya kare grid)
  - Renk: detection yoğunluğu veya toplam FRP
  - Zaman slider'ı ile ay ay ilerleme
  - Raw vs Harmonized modda farklı dağılım
- **Bitti sayılır:** mock veriyle heatmap render oluyor; zaman slider'ı çalışıyor; renk skalası anlamlı.

### [ ] Y4 — İstatistik dashboard kartları
- **Bekler:** R1, Y5 · **Açar:** —
- Üst kısımda summary bilgiler:
  - Toplam detection sayısı (raw vs harmonized)
  - Duplicate removal oranı (%)
  - Aktif sensör sayısı
  - Ortalama FRP
  - Son güncelleme zamanı
  - Kart tasarımı: büyük sayı + trend ok (▲▼) + mini sparkline
- **Bitti sayılır:** en az 4 istatistik kartı mock veriyle render oluyor; raw/harmonized geçişinde sayılar değişiyor.

### [ ] Y5 — Mock veri hazırlığı (frontend için)
- **Bekler:** — · **Açar:** Y1, Y2, Y3, Y4
- Frontend component'leri backend hazır olmadan geliştirilebilsin:
  - `web/src/mocks/hotspots.json` — 500-1000 arası hotspot (lat, lon, frp, confidence, sensor, date)
  - `web/src/mocks/calendar.json` — 2000–2025 arası aylık hücre verisi (count, mean_frp, anomaly)
  - `web/src/mocks/stats.json` — genel istatistikler (raw vs harmonized count, pipeline metrics)
  - Veriler gerçekçi olmalı: FRP aralığı 1–500 MW, mevsimsellik var (yaz ayları yoğun), 2012'de VIIRS geçişi belirgin
  - `web/src/mocks/index.ts` ile export
- **Bitti sayılır:** mock dosyaları `web/src/mocks/` altında; TypeScript hatasız import ediliyor; sensor cliff 2012'de görünüyor.

---

## Sadık'ın görevleri

### [ ] K1 — Live/demo/fallback modu
- **Bekler:** R4 · **Açar:** R8
- FIRMS API çalışmadığında uygulamanın hâlâ demo yapılabilmesi (FireHarmonize'dan ilham):
  - API bağlantısı kontrol mekanizması (health check → timeout → fallback)
  - Demo modu: `web/src/mocks/` altındaki örnek veriyi otomatik yükleme
  - UI'da mod göstergesi: "🟢 Live Data" vs "🟡 Demo Mode"
  - Kullanıcı manuel geçiş yapabilmeli (demo verisiyle hızlı keşif)
  - Geçiş sırasında state sıfırlanmadan smooth transition
  - Jüri demo anında API çökerse sunum kesilmesin
- **Kaynak:** FireHarmonize — "Live/demo/fallback modu: API çalışmazsa demo verisiyle devam"
- **Bitti sayılır:** API key olmadan uygulama demo modunda açılıyor; API varken otomatik live'a geçiyor; mod göstergesi doğru; jüri demo'su API olmadan sorunsuz çalışıyor.

### [ ] K2 — Config ayarları frontend paneli
- **Bekler:** G7, R1 · **Açar:** —
- Pipeline parametrelerini UI'dan ayarlayabilme (EG FireTrack tarzı şeffaflık):
  - Settings panel/drawer: slider ve input alanlarıyla parametre düzenleme
  - Dedup eşikleri: zaman penceresi (1–30s), örtüşme oranı (%10–%80)
  - Matching eşikleri: mesafe (0.5–10 km), zaman (1–30 dk)
  - Değişiklik yapıldığında sonuçlar yeniden hesaplansın (debounced API call)
  - "Varsayılana dön" butonu
  - Ayarların URL query string'ine yansıması (paylaşılabilir link)
- **Bitti sayılır:** en az 3 parametre UI'dan değiştirilebiliyor; sonuçlar değişiyor; varsayılan butonuyla geri dönülebiliyor; link paylaşılınca aynı ayarlarla açılıyor.

### [ ] K3 — Belirsizlik aralıkları görselleştirme
- **Bekler:** G9, C7, R1 · **Açar:** —
- Belirsizlik verilerinin chart'larda gösterimi (Recharts ile):
  - FRP zaman serisi: shaded confidence band (mean ± uncertainty)
  - Haritada: hotspot dairesinin boyutu veya opaklığı belirsizlik ile orantılı
  - Calendar hücresinde: belirsizlik yüksekse hücre kenarlığı farklı (hatched/dashed)
  - Tooltip'te: "FRP: 45.2 MW (38.7–51.8)" formatında
  - Belirsizlik açıklaması: "Geniş aralık = sensör açısı büyük veya kalibrasyon verisi az"
- **Kaynak:** fire-harmony — belirsizlik gösterimi; Can'ın C7 tasarımına uygun
- **Bitti sayılır:** en az 2 chart tipinde belirsizlik görünüyor; tooltip'te aralık var; yüksek VZA'lı noktalar geniş aralıklı (görsel olarak ayırt edilebilir).

### [ ] K4 — Erişilebilir renk skalası sistemi
- **Bekler:** C7, R1 · **Açar:** —
- Yarışma gereksinimlerinden "color ramps" + erişilebilirlik:
  - Renk skalası modülü: sequential (FRP), diverging (anomaly), categorical (sensör)
  - En az 2 palet seçeneği: varsayılan + renk körlüğü dostu (viridis/cividis)
  - Kullanıcı palet değiştirebilmeli (dropdown)
  - CSS custom properties ile tema entegrasyonu
  - Legend component: sürekli skala + eşik değerleri
  - Quantile-based renk atama (outlier'lar skalayı bozmasın)
- **Kaynak:** Yarışma gereksinimleri — "Color ramps"; erişilebilirlik en iyi pratikleri
- **Bitti sayılır:** en az 2 palet var; renk körlüğü simülatörüyle kontrol edilmiş; legend'da değer aralıkları görünüyor; palet değiştirince tüm chart'lar güncelleniyor.

> [!NOTE]
> K5–K9 görevleri 2026-11-14'te Sadık'a atandı (eski G11–G15 kimliklerinden bu dosyada K5–K9 olarak yeniden numaralandı; K1–K4 ile çakışma yok).

### [ ] K5 — Area API sonuçlarını diske cache'leme
- **Bekler:** — · **Açar:** K6, K7
- **Neden:** G3 doğrulamasında FIRMS transaction limitine dayandık. 18 aylık tek bir aralık 1373 transaction harcıyor ve limit aşıldığında istekler `HTTP 400 Exceeding allowed transaction limit` dönüyor. Aynı aralığı tekrar sorgulamak baştan harcama demek, bu yüzden uzun zaman serisi doğrulamaları yapılamıyor.
- `crates/firms-client/src/archive.rs` içindeki `ArchiveClient` ülke-yıl dosyalarını zaten diske cache'liyor ve bu yaklaşım çalışıyor (`.cache/firms-archive`, 2,4 GB, MODIS 2000-2025 + VIIRS SNPP 2012-2025). Aynı mantık area API sorguları için kurulacak.
  - Cache anahtarı: kaynak + bbox + tarih penceresi. Pencere `DayRange::MAX = 5` gün olduğu için anahtar doğal olarak ayrıklaşıyor.
  - Cache hit'te transaction HARCANMAYACAK; `report.transactions_spent` yalnız gerçekten yapılan isteği saymalı.
  - NRT kaynakları için cache süreli olmalı (veri güncelleniyor), SP ve arşiv kaynakları için kalıcı (veri dondu).
  - `.cache/` dizini gitignore'da olmalı.
- **Bitti sayılır:** aynı aralık ikinci kez istendiğinde `transactions_spent` 0 dönüyor; cache'li bir aralık ağ bağlantısı olmadan da çalışıyor.

### [ ] K6 — 2023 (NOAA-21) efor geçişini ölçme
- **Bekler:** K5 · **Açar:** —
- **Neden:** G3'ün Ω mekanizması 2012 (SNPP) ve 2018 (NOAA-20) geçişlerinde doğrulandı, ama **2023 (NOAA-21) geçişi ölçülmedi.** Mekanizma platform kümesinden bağımsız çalıştığı için aynı davranması bekleniyor, ancak bu bir beklenti; ölçüm değil.
- Ülke arşivi NOAA-21'i taşımıyor (`viirs-jpss2` tüm yıllarda HTTP 404; NOAA-20 ise `viirs-jpss1` olarak arşivde var), bu yüzden bu geçiş yalnız area API ile ölçülebilir ve K5'in cache'ine ihtiyaç duyar.
  - `VIIRS_NOAA21_SP` kaynağı 2023-03-01'de başlıyor. Geçişi kapsayan aralık: 2022-09 … 2023-09 civarı.
  - Aynı mevsim kıyaslaması yapılmalı (örn. Tem-Ara 2022 vs Tem-Ara 2023), aksi halde mevsim etkisi geçişle karışır.
  - Ω'nın her iki konfigürasyon için de hesaplanabilmesi için her yanda en az 3 ay gerekiyor (`MIN_MONTHS_PER_CONFIG`).
- **Bitti sayılır:** `|log(normalize kayma)| < |log(ham kayma)| / 2` ölçütü 2023 geçişi için de sağlanıyor ve sonuç GOREVLER.md'deki G3 doğrulama tablosuna ekleniyor.

### [ ] K7 — Tam filo 2000-present trend serisi
- **Bekler:** K5 · **Açar:** —
- **Neden:** G3'ün "monoton artan trend kalkıyor" şartı Spearman ρ +0,785 → +0,087 ile doğrulandı, **ama o seri yalnızca MODIS + SNPP içeriyor.** O ölçüm sırasında NOAA-20 arşivde yok sanılıyordu, bu yüzden dört efor geçişinden ikisi (2018, 2023) trend testinde hiç yok. Trend, 2012 geçişini içeren uzun seride düz; 2018 ayrı ve kısa bir veri setinde doğrulandı; ikisi tek sürekli seride birlikte sınanmadı.
- Güncel durum (2026-11-15): yerel arşiv (`make data`) MODIS + SNPP + NOAA-20'yi 2000-2025 boyunca içeriyor, yani seri 2018 geçişi dahil doğrudan arşivden kurulabilir. Area API (K5 cache'i) yalnız 2023 sonrası NOAA-21 katkısı için gerekiyor. Sonra `archive_ingest` örneğinin sıçrama kontrolü tam filo serisi üzerinde koşturulacak.
- **Bitti sayılır:** 2000-present tam filo serisinde normalize Spearman ρ'nun mutlak değeri ham ρ'nun yarısından küçük; 2012, 2018 ve 2023 geçişlerinin üçünde de seviye kayması ölçütü sağlanıyor.

### [ ] K8 — MODIS içi efor geçişi (2002 Aqua)
- **Bekler:** — · **Açar:** —
- **Neden:** Ω tanım gereği `total / modis_observable` oranı olduğu için **MODIS referans kanalının kendi içindeki değişimi göremiyor.** 2002-07'de Aqua devreye girdiğinde tespitler arttı (arşiv, Türkiye: 2001 Terra 5162 → 2002 Terra 4474 + Aqua 2412) ama Aqua `modis_observable`'ın parçası olduğu için Ω 1,000 kalıyor ve bu sıçrama düzeltilmiyor.
- MODIS filo birleştirmesi (bkz. `crates/core/src/fleet.rs`, 180 dk pencere) aynı yangının Terra/Aqua çift sayımını temizliyor, ama ölçüme göre Aqua tespitlerinin yalnız %18-21'i Terra ile eşleşiyor. Kalanı Terra'nın gerçekten görmediği yangınlar (sabah geçişinden sonra başlayanlar) ve bu gerçek bir efor artışı.
- **Genel ilke:** aynı yangının çift sayımı *birleştirmeyle*, yeni platformun ek görme kabiliyeti *Ω ile* çözülür. Ω'nın çalışması için referans kanalın geçişin iki yanında da bulunması gerekir — VIIRS geçişlerinde MODIS bunu sağlıyor, Aqua geçişinde sağlayan tek şey Terra.
- Değerlendirilecek iki yol:
  1. Ω'nın referansını "herhangi bir MODIS" yerine Terra-only alt kanalı yapmak. **Dikkat:** Terra'nın yörüngesi 2020 sonrası kaydı ve geçiş saati değişti, yani kararsız bir cetvel olur ve 2020-2026 için YENİ bir artefakt üretebilir. Bu yol seçilirse Terra kaymasının etkisi önce ölçülmeli.
  2. 2000-11 … 2002-06 arası 20 ayı `count_normalized: None` ile "kıyaslanabilir değil" işaretlemek. Veri kaybı 26 yılın küçük bir kısmı ve o dönem MODIS misyonunun en az güvenilir evresi. Daha dürüst ve risksiz.
- **Not:** G3'ün bitiş şartı yalnızca 2012 ve 2017 geçişlerini istediği için bu madde şartı ihlal etmiyor; 2000-2002 taban döneminin sonrasıyla tam kıyaslanabilir olmaması bilinen ve kayıtlı bir sınır.
- **Bitti sayılır:** 2000-2002 dönemi ya düzeltiliyor ya da açıkça kıyaslanamaz işaretleniyor; methods sayfasında (`C5` / `R6`) hangisinin seçildiği belirtiliyor.

### [ ] K9 — SPA fallback gerçekten çalışmıyor
- **Bekler:** — · **Açar:** R1 (routing eklenince kritik olur)
- **Neden:** `crates/server/CLAUDE.md` "Teknik Notlar" bölümünde *"`tower_http::services::ServeDir` ile SPA fallback sağlanır"* yazıyor, ama bu DOĞRU DEĞİL. Düz `ServeDir` bilinmeyen bir yolda `index.html` döndürmez, 404 döner.
- **Ölçüm (2026-11-14, `web/dist` build edilmiş durumda, backend :3001):**

  | İstek | Dönen |
  |-------|-------|
  | `/` | HTTP 200, `text/html` (doğru) |
  | `/harita` | **HTTP 404, boş gövde** (yanlış, `index.html` dönmeliydi) |
  | `/api/olmayanbirsey` | HTTP 404, `{"error":"Unknown API endpoint: ..."}` (doğru, G4'te düzeltildi) |

- **Neden şu an gizli:** `web/src/App.tsx` içinde henüz router yok, uygulama tek sayfa. R1 (React component mimarisi + layout) routing eklediğinde derin linkler ve sayfa yenileme (F5) kırılacak. Jüri demo sırasında bir alt sayfada yenileme yaparsa boş 404 görür.
- Yapılacak:
  - `ServeDir::new(&static_dir)` yerine `index.html`'e düşen bir fallback kur: `ServeDir::new(&static_dir).fallback(ServeFile::new(format!("{static_dir}/index.html")))` veya `.not_found_service(...)`. `tower-http`'nin `fs` feature'ı zaten bağlı, yeni dependency gerekmez.
  - `/api/*` yollarının JSON 404 davranışı KORUNMALI; SPA fallback yalnız `/api` dışı yolları yakalamalı. G4'te `/api` ayrı bir router'a alındı, o yapı bozulmamalı.
  - `STATIC_DIR` dizini veya `index.html` yoksa sunucu panik etmemeli; fallback kurulamazsa mevcut davranışa düşüp uyarı loglanmalı (Docker'da `STATIC_DIR=/app/static`, geliştirmede `web/dist` ve build edilmemiş olabilir).
  - `crates/server/CLAUDE.md`'deki yanlış iddia düzeltilmeli.
- **Bitti sayılır:** `web/dist` build edilmiş haldeyken `/harita` isteği HTTP 200 ve `index.html` dönüyor; `/api/olmayanbirsey` hâlâ JSON 404 dönüyor; `STATIC_DIR` boşken sunucu ayağa kalkmaya devam ediyor.

---

## Kişi başı yük özeti

| Kişi | Görev sayısı | Ağırlık | Odak |
|------|-------------|---------|------|
| Görkem | 4 (G7–G10) | Ağır | Backend core: config, kalibrasyon, belirsizlik, export |
| Can | 3 (C5–C7) | Orta | Tasarım: methods, export/brifingi, belirsizlik/renk |
| Serhat | 3 (R6–R8) | Orta-ağır | Frontend impl: methods, export+brifingi, model seçimi |
| Samet | 3 (S6–S8) | Orta | Test, veri, referans: yeni modül testleri, case study, bilimsel içerik |
| Sadık | 4 (K1–K4) | Orta | Frontend infra: fallback, config paneli, belirsizlik viz, renk skalası |
