/** Copy for the 60-second guided tour; merged into the main dictionaries in copy.ts. */

export const tourCopyEn = {
  tourStart: "60-s tour",
  tourStartDescription: "Start a 60-second guided tour of the Raw to Harmonized story",
  tourStepOf: "Step {n} of {total}",
  tourTimeWindow: "{from}–{to} s",
  tourBack: "Back",
  tourNext: "Next",
  tourFinish: "Finish",
  tourPause: "Pause",
  tourResume: "Resume",
  tourSkip: "Skip tour",
  tourAnnounce: "Step {n} of {total}: {title}",
  tourStepToggleTitle: "Raw ↔ Harmonized",
  tourStepToggleBody:
    "Raw detection counts jump when S-NPP (2012) and NOAA-20 (2018) join the fleet: the satellites changed, not the fires. Watch the switch flip to Harmonized: duplicate detections merge and the sensor cliff flattens. Press H to flip it yourself.",
  tourStepLensTitle: "Footprint Lens",
  tourStepLensBody:
    "A MODIS pixel covers 1 km² at nadir and about 9.7 km² at the scan edge; a VIIRS pixel is 375 m. Click a MODIS point on the map, then “{lens}”, to see the VIIRS pixels inside one MODIS footprint and the detection thresholds: MODIS 5.7→40 MW vs VIIRS 0.7→3.7 MW. That is why one MODIS detection can equal N VIIRS detections.",
  tourStepCaseTitle: "Policy case: China's straw-burning ban",
  tourStepCaseBody:
    "China banned open straw burning in 2013. Raw detections rise 10% from 2012 to 2018, as if the ban failed. The harmonized series drops 32%, and S-NPP alone drops 24%. Burning moved in season: in Northeast China and the North China Plain the December–May share of fire energy rose from 27% to 73%.",
  tourStepValidationTitle: "Live validation",
  tourStepValidationBody:
    "Placebo step test: at the 2012 and 2018 sensor seams the harmonized count is indistinguishable from ordinary months (mean rank 0.49 and 0.51), while raw counts stand out (p = 0.002–0.039). Leave-one-year-out: the effort normalization error is 0.052 vs 1.79 without it, and the model beats both baselines in 10 of 10 countries. Below, the ±1σ Mondrian (conformal) band and the Orbital Drift strip are explained, and the limits of the method are listed.",
} as const;

export const tourCopyTr = {
  tourStart: "60 sn tur",
  tourStartDescription: "Ham'dan Harmonize'ye hikâyenin 60 saniyelik rehberli turunu başlat",
  tourStepOf: "Adım {n} / {total}",
  tourTimeWindow: "{from}–{to} sn",
  tourBack: "Geri",
  tourNext: "İleri",
  tourFinish: "Bitir",
  tourPause: "Duraklat",
  tourResume: "Devam et",
  tourSkip: "Turu atla",
  tourAnnounce: "Adım {n} / {total}: {title}",
  tourStepToggleTitle: "Ham ↔ Harmonize",
  tourStepToggleBody:
    "S-NPP (2012) ve NOAA-20 (2018) filoya katıldığında ham tespit sayısı sıçrıyor: değişen yangınlar değil, uydular. Anahtarın Harmonize'ye geçişini izleyin: mükerrer tespitler birleşiyor ve sensör uçurumu düzleşiyor. Kendiniz denemek için H tuşuna basın.",
  tourStepLensTitle: "Ayak İzi Merceği",
  tourStepLensBody:
    "Bir MODIS pikseli nadirde 1 km², tarama kenarında yaklaşık 9,7 km² alanı kaplar; VIIRS pikseli 375 m'dir. Haritada bir MODIS noktasına, ardından “{lens}” düğmesine tıklayarak tek bir MODIS ayak izinin içindeki VIIRS piksellerini ve tespit eşiklerini görün: MODIS 5,7→40 MW, VIIRS 0,7→3,7 MW. Bu yüzden bir MODIS tespiti N VIIRS tespitine denk gelebilir.",
  tourStepCaseTitle: "Politika vakası: Çin'in anız yakma yasağı",
  tourStepCaseBody:
    "Çin 2013'te açık anız yakmayı yasakladı. Ham tespitler 2012'den 2018'e %10 artıyor; sanki yasak işe yaramamış. Harmonize seri %32 düşüyor, tek başına S-NPP de %24 düşüş gösteriyor. Yakma mevsim değiştirdi: Kuzeydoğu Çin ve Kuzey Çin Ovası'nda yangın enerjisinin Aralık–Mayıs payı %27'den %73'e çıktı.",
  tourStepValidationTitle: "Canlı doğrulama",
  tourStepValidationBody:
    "Placebo adım testi: 2012 ve 2018 sensör dikişlerinde harmonize sayım sıradan aylardan ayırt edilemiyor (ortalama sıra 0,49 ve 0,51), ham sayım ise belirgin biçimde ayrışıyor (p = 0,002–0,039). Yıl-dışarıda (LOYO) testi: efor normalizasyonunun hatası 0,052, normalizasyonsuz 1,79; model 10 ülkenin 10'unda iki baseline'ı da yeniyor. Aşağıda ±1σ Mondrian (konformal) bandı ve Yörünge Sürüklenmesi şeridi açıklanıyor, yöntemin sınırları listeleniyor.",
} as const;
