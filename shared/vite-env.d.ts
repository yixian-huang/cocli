// Minimal ImportMeta augmentation so shared/ compiles standalone without
// the full vite/client types. The real declaration lives in web/src/vite-env.d.ts.
interface ImportMetaEnv {
  readonly VITE_API_BASE?: string
  readonly VITE_USE_MOCK?: string
  [key: string]: string | undefined
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
