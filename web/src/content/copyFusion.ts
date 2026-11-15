/** Harmonization-model copy fragment merged into copy.ts. Keys here must not collide with copy.ts keys. */

export const fusionCopyEn = {
  fusionTitle: "Harmonization model",
  fusionNote:
    "Changes how matched MODIS and VIIRS records are combined for the selected month. The yearly chart and calendar stay on the Standard model.",
  fusionViirsPreferred: "Standard (VIIRS-preferred)",
  fusionViirsPreferredHint: "Matched records take the VIIRS FRP.",
  fusionEqualWeight: "Equal-weight",
  fusionEqualWeightHint: "Matched records average the angle-corrected MODIS FRP and the VIIRS FRP.",
  fusionModisCalibrated: "MODIS-calibrated",
  fusionModisCalibratedHint:
    "MODIS is the reference: matched records take the angle-corrected MODIS FRP, and weak VIIRS-only detections are dropped.",
  fusionComparison:
    "This model gives {count} records vs {baseline} with Standard ({delta} records), total FRP {frpDelta}.",
  fusionDropped: "{n} VIIRS-only detections below the MODIS detection limit were dropped.",
  fusionBadge: "Model: {model}",
  fusionBadgeAria: "Harmonization model {model} active. Open settings",
} as const;

export const fusionCopyTr: { readonly [K in keyof typeof fusionCopyEn]: string } = {
  fusionTitle: "Harmonizasyon modeli",
  fusionNote:
    "Seçili ay için eşleşen MODIS ve VIIRS kayıtlarının nasıl birleştirileceğini değiştirir. Yıllık grafik ve takvim Standart modelde kalır.",
  fusionViirsPreferred: "Standart (VIIRS öncelikli)",
  fusionViirsPreferredHint: "Eşleşen kayıtlar VIIRS FRP değerini alır.",
  fusionEqualWeight: "Eşit ağırlık",
  fusionEqualWeightHint: "Eşleşen kayıtlar, açı düzeltmeli MODIS FRP ile VIIRS FRP değerinin ortalamasını alır.",
  fusionModisCalibrated: "MODIS kalibreli",
  fusionModisCalibratedHint:
    "MODIS referanstır: eşleşen kayıtlar açı düzeltmeli MODIS FRP değerini alır, zayıf yalnızca-VIIRS tespitleri atılır.",
  fusionComparison:
    "Bu model Standart modeldeki {baseline} kayıt yerine {count} kayıt verir ({delta} kayıt), toplam FRP {frpDelta}.",
  fusionDropped: "MODIS tespit sınırının altındaki {n} yalnızca-VIIRS tespiti atıldı.",
  fusionBadge: "Model: {model}",
  fusionBadgeAria: "Harmonizasyon modeli {model} etkin. Ayarları aç",
};
