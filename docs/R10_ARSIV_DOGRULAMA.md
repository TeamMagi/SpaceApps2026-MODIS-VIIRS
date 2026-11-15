# R10 — Gerçek arşivde ayak izi toplamı doğrulaması

15 Kasım 2026. Çevrimdışı FIRMS ülke arşivi; MODIS, S-NPP ve NOAA-20. Varsayılan `PipelineConfig`; önce tüm yılın vejetasyon yangınlarından statik maske kuruldu, ardından seçili aylar işlendi. Ağ isteği ve API işlem harcaması yoktur.

```bash
cargo run --release --locked -p pyrosync-firms --example r10_archive_check
```

Makine çıktısı: [r10_archive_check.json](test/r10_archive_check.json). Arşiv yoksa komut açık hata verir; sentetik veriye düşmez.

## Yöntem

Denetleyici üretim pipeline'ının çıktı bağlantılarını ham arşiv satırlarıyla ilişkilendirir. Dedup ve filo birleştirme aşamaları aynı genel API üzerinden yeniden kurulur; her `Matched` kaydı için bağlı VIIRS filo gözlemlerinin FRP ortalamaları toplanarak çıktı FRP'siyle karşılaştırılır. Böylece kardeş platformların aynı pikseli iki kez toplamaya dönüşmesi önlenir: önce filo ortalaması, sonra farklı alt piksellerin toplamı.

Eşleştirme H3 aday aramasına bağımsız olarak, zaman penceresindeki bütün MODIS filo gözlemleri taranarak en yakın sahip yeniden bulunur. Bu sahibin `Matched` kaydıyla aynı geçişte (tohumdan en fazla 600 saniye), aynı MODIS ayak izi içinde kalan herhangi bir VIIRS filo gözlemi `ViirsOnly` kaldıysa denetim başarısızdır. Farklı geçişteki veya başka MODIS'e ait `ViirsOnly` kayıtlar hata sayılmaz.

Ek piksellerin ayak izi ve zaman sınırı, en yakın MODIS sahipliği, filo grubunun tüm üyelerinin aynı çıktıya bağlanması, aşama sayımı ve toplam VIIRS FRP korunumu da kontrol edilir. Tek FRP toplamı ve genel enerji korunumu için tolerans `1e-9 × max(1, toplam)` MW; diğer kontroller sıfır ihlal ister. Çoklu piksel örneği yoksa komut başarısız olur.

## Sonuç

| Bölge / ay | Ham kayıt | Matched | Çoklu piksel Matched | Toplanan ek VIIRS | En çok piksel | Uygun ViirsOnly artığı | İhlal |
|---|---:|---:|---:|---:|---:|---:|---:|
| Kıbrıs 2021-07 | 228 | 15 | 7 | 14 | 4 | 0 | 0 |
| Türkiye 2021-07 | 13.648 | 870 | 537 | 1.549 | 20 | 0 | 0 |
| Türkiye 2021-08 | 16.058 | 1.067 | 594 | 1.607 | 36 | 0 | 0 |
| Angola 2019-04 | 14.709 | 675 | 254 | 426 | 9 | 0 | 0 |
| **Toplam** | **44.643** | **2.627** | **1.392** | **3.596** | **36** | **0** | **0** |

Her örnekte ek piksellerin ayak izi dışına çıkma sayısı sıfır. En büyük tek `Matched` FRP farkı `2,274e-13` MW; toplam VIIRS FRP korunumu farkı en çok `5,239e-10` MW (kayan nokta toplama sırası düzeyi).

## Sınırlar ve karar

Bu dört dönemlik örneklem R10'un gerçek veri kabul şartını karşılar; küresel doğruluk veya fiziksel radyometrik kalibrasyon kanıtı değildir. Ayak izi, mevcut modelde scan/track yarı köşegeniyle sınırlı bir dairedir (en az 0,75 km, en çok eşleştirme yarıçapı). Tohum eşleşmesi 2 km eşleştirme yarıçapında seçilebilir; ayak izi sınırı ek piksellere uygulanır. Dolayısıyla “bütün eşleşmeler fiziksel piksel dikdörtgeninin içinde” iddiası yapılmaz.

Üretim harmonizasyon kodu, config ve seri şeması değişmedi; yeni precompute gerekmiyor. R10 kodu zaten main'de ve G6'nın şema 10 hazır sonuçlarında. Denetleyici, workspace testleri ve `clippy --all-targets -D warnings` geçti; R10 kapatıldı.
