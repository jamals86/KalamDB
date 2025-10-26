/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_KALAMDB_URL: string
  readonly VITE_KALAMDB_API_KEY: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
