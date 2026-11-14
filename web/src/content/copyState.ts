/** state copy fragment merged into copy.ts. Keys here must not collide with copy.ts keys. */

export const stateCopyEn = {
  stateErrorUnreachable: "The PyroSync data service is unreachable. Start the backend and try again.",
  stateErrorServerStatus: "Data server returned an error (HTTP {status}).",
  stateErrorInvalidResponse: "The data service returned an invalid response.",
  stateErrorRegionTooLarge: "This region is too large to process on this server.",
  stateErrorRegionsEmpty: "The server reports no regions in the archive.",
  stateErrorBoundaryTitle: "PyroSync could not render this view",
  stateErrorBoundaryBody: "An unexpected error interrupted the application. Reload the page to start fresh.",
  receiptPeriodAngle: "FRP corrected for angle (shown period)",
} as const;

export const stateCopyTr: { readonly [K in keyof typeof stateCopyEn]: string } = {
  stateErrorUnreachable: "PyroSync veri servisine ulaşılamıyor. Sunucuyu başlatıp tekrar deneyin.",
  stateErrorServerStatus: "Veri sunucusu hata döndürdü (HTTP {status}).",
  stateErrorInvalidResponse: "Veri servisi geçersiz bir yanıt döndürdü.",
  stateErrorRegionTooLarge: "Bu bölge bu sunucuda işlenmek için çok büyük.",
  stateErrorRegionsEmpty: "Sunucu arşivde hiç bölge olmadığını bildiriyor.",
  stateErrorBoundaryTitle: "PyroSync bu görünümü oluşturamadı",
  stateErrorBoundaryBody: "Beklenmeyen bir hata uygulamayı durdurdu. Yeniden başlatmak için sayfayı yenileyin.",
  receiptPeriodAngle: "Bakış açısına göre düzeltilmiş FRP (gösterilen dönem)",
};
