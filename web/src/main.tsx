import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router-dom'
import './index.css'
import { router } from './router'
import { ErrorBoundary } from '@/components/ErrorBoundary'
import { GlobalLoadingBar } from '@/components/ui'
import { setUnauthorizedHandler } from '@/api/client'
import { useUserStore } from '@/stores/userStore'
import '@/i18n'

// Centralized 401 handler: clear auth state and bounce to /login. Guarded so
// repeated 401s while already on /login do not push duplicate history entries.
setUnauthorizedHandler(() => {
  if (useUserStore.getState().user === null) return
  useUserStore.getState().logout()
  if (window.location.pathname !== '/login') {
    router.navigate('/login', { replace: true })
  }
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
