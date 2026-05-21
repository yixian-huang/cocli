import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router-dom'
import './index.css'
import { router } from './router'
import { ErrorBoundary } from '@/components/ErrorBoundary'
import { GlobalLoadingBar } from '@/components/ui'
import { setUnauthorizedHandler } from '@/api/client'
import '@/i18n'

// Single-tenant OSS: no login page. A 401 means the API key is wrong; nothing
// to navigate away to. Handler is a no-op to satisfy the client contract.
setUnauthorizedHandler(() => {
  // no-op
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ErrorBoundary>
      <GlobalLoadingBar />
      <RouterProvider router={router} />
    </ErrorBoundary>
  </StrictMode>,
)

if ('serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').catch((err) => console.warn('[sw] registration failed:', err))
  })
}
