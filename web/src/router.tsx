import { lazy, Suspense, useEffect } from 'react'
import { Outlet, createBrowserRouter } from 'react-router-dom'
import App from './App'
import { ChannelRoute } from './routes/ChannelRoute'
import { LoginRoute } from './routes/LoginRoute'
import { ZonePanelRoute } from './routes/ZonePanelRoute'
import { ZoneDevToolsRoute } from './routes/ZoneDevToolsRoute'
import { LegacyDevtoolsRedirect } from './routes/LegacyDevtoolsRedirect'
import { Skeleton } from './components/ui/Skeleton'
import { useUserStore } from '@/stores/userStore'

const AgentRoute = lazy(() => import('./routes/AgentRoute').then(m => ({ default: m.AgentRoute })))
const InviteRoute = lazy(() => import('./routes/InviteRoute').then(m => ({ default: m.InviteRoute })))
const DevToolsPage = lazy(() => import('./components/devtools/DevToolsPage').then(m => ({ default: m.DevToolsPage })))
const DaemonManagePage = lazy(() => import('./components/daemons/DaemonManagePage').then(m => ({ default: m.DaemonManagePage })))

function LazyFallback() {
  return <div className="flex-1 flex items-center justify-center"><Skeleton variant="rectangle" width="100%" height="200px" /></div>
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
        path: '/invite/:code',
        element: <Suspense fallback={<LazyFallback />}><InviteRoute /></Suspense>,
      },
      {
        path: '/login',
        element: <LoginRoute />,
      },
      {
        path: '/',
        element: <App />,
        children: [
          { index: true, element: <ChannelRoute /> },
          { path: 'channel/:channelId', element: <ChannelRoute /> },
          { path: 'channel/:channelId/msg/:id', element: <ChannelRoute /> },
          { path: 'agent/:id', element: <Suspense fallback={<LazyFallback />}><AgentRoute /></Suspense> },
          { path: 'devtools', element: <LegacyDevtoolsRedirect /> },
          { path: 'devtools/daemon/:machineId', element: <LegacyDevtoolsRedirect /> },
        ],
      },
      {
        path: '/z/:zoneSlug',
        element: <App />,
        children: [
          { index: true, element: <ChannelRoute /> },
          { path: 'channel/:channelId', element: <ChannelRoute /> },
          { path: 'channel/:channelId/msg/:id', element: <ChannelRoute /> },
          { path: 'history', element: <ZonePanelRoute panel="history" /> },
          { path: 'tasks', element: <ZonePanelRoute panel="zone_tasks" /> },
          { path: 'members', element: <ZonePanelRoute panel="zone_members" /> },
          { path: 'keys', element: <ZonePanelRoute panel="zone_credentials" /> },
          { path: 'devtools', element: <ZoneDevToolsRoute><Suspense fallback={<LazyFallback />}><DevToolsPage /></Suspense></ZoneDevToolsRoute> },
          { path: 'devtools/daemon/:machineId', element: <LegacyDevtoolsRedirect /> },
          { path: 'daemons', element: <ZoneDevToolsRoute><Suspense fallback={<LazyFallback />}><DaemonManagePage /></Suspense></ZoneDevToolsRoute> },
          { path: 'daemons/:machineId', element: <ZoneDevToolsRoute><Suspense fallback={<LazyFallback />}><DaemonManagePage /></Suspense></ZoneDevToolsRoute> },
          { path: 'agent/:id', element: <Suspense fallback={<LazyFallback />}><AgentRoute /></Suspense> },
        ],
      },
    ],
  },
])
