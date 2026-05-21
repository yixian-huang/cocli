/// <reference types="vite/client" />

interface ImportMetaEnv {
  /**
   * Optional absolute origin used to prefix all REST requests, e.g.
   * "https://api.example.com". When unset (the default), requests use
   * relative paths so the same bundle works behind any reverse proxy.
   */
  readonly VITE_API_BASE?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
