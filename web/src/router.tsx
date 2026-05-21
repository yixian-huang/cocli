import { lazy, Suspense, useEffect } from 'react'
import { Navigate, Outlet, createBrowserRouter } from 'react-router-dom'
import App from './App'
import { ChannelRoute } from './routes/ChannelRoute'
import { Skeleton } from './components/ui/Skeleton'
import { useUserStore } from '@/stores/userStore'

const AgentRoute = lazy(() =>
  import('./routes/AgentRoute').then((m) => ({ default: m.AgentRoute })),
)
const SettingsPluginsRoute = lazy(() =>
  import('./routes/SettingsPluginsRoute').then((m) => ({ default: m.SettingsPluginsRoute })),
)

function LazyFallback() {
  return (
    <div className="flex-1 flex items-center justify-center">
      <Skeleton variant="rectangle" width="100%" height="200px" />
    </div>
  )
}

function RootLayout() {
  const init = useUserStore((s) => s.init)
  useEffect(() => {
    init()
  }, [init])
  return <Outlet />
}

export const router = createBrowserRouter([
  {
    element: <RootLayout />,
    children: [
      {
        path: '/',
        element: <App />,
        children: [
          { index: true, element: <ChannelRoute /> },
          { path: 'channel/:channelId', element: <ChannelRoute /> },
          { path: 'channel/:channelId/msg/:id', element: <ChannelRoute /> },
          {
            path: 'agent/:id',
            element: (
              <Suspense fallback={<LazyFallback />}>
                <AgentRoute />
              </Suspense>
            ),
          },
          {
            path: 'settings/plugins',
            element: (
              <Suspense fallback={<LazyFallback />}>
                <SettingsPluginsRoute />
              </Suspense>
            ),
          },
          { path: '*', element: <Navigate to="/" replace /> },
        ],
      },
    ],
  },
])
