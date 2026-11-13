# web — React Frontend

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- TypeScript derleyicisi **SWC** olmalıdır (`@vitejs/plugin-react-swc`). Babel kullanılmaz.
- Strict TypeScript: `"strict": true`, `"noUnusedLocals": true`, `"noUnusedParameters": true`.

## Dizin Yapısı

```
src/
├── components/     → UI bileşenleri (her bileşen kendi klasöründe)
│   ├── Layout/     → Sayfa düzeni (AppShell)
│   ├── Toggle/     → Raw ↔ Harmonized geçiş butonu
│   └── Stats/      → İstatistik kartları
├── hooks/          → Custom React hooks (useApi vb.)
├── services/       → API çağrı katmanı
├── types/          → TypeScript tip tanımları (backend ile senkron)
├── mocks/          → Geliştirme için örnek veri setleri
├── App.tsx         → Ana uygulama bileşeni
└── main.tsx        → React root
```

## Bileşen Kuralları

- Her bileşen kendi alt dizininde bulunur: `components/FeatureName/FeatureName.tsx`.
- Props interface'leri bileşen dosyasında tanımlanır.
- Shared tipler `types/index.ts`'te tutulur.
- API çağrıları doğrudan bileşende yapılmaz, `services/` katmanı kullanılır.
- State yönetimi için custom hook'lar `hooks/` dizininde oluşturulur.

## Geliştirme

- `npm run dev` → Vite dev server (port 5173), `/api` istekleri localhost:3001'e proxy edilir.
- `npm run build` → Production build (`dist/` dizinine).
- Yeni dependency eklerken SWC uyumluluğu kontrol edilir.

## Stil

- Şimdilik inline style kullanılıyor, ileride CSS Modules veya Tailwind'e geçilebilir.
- CSS custom properties (variables) ile tema desteği sağlanır.
- Responsive tasarım: mobile-first yaklaşım.

## Grafik Kütüphanesi

- Veri görselleştirme için **Recharts** kullanılacaktır.
- Chart bileşenleri `components/Charts/` altında oluşturulur.
