import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { BrowserRouter as Router, Routes, Route, NavLink } from 'react-router-dom'
import { Home, Database, Route as RouteIcon, Settings } from 'lucide-react'
import { Dashboard } from './pages/Dashboard'
import { Clusters } from './pages/Clusters'
import { Routes as RoutesPage } from './pages/Routes'
import { Login } from './pages/Login'
import { AuthProvider } from './lib/auth-context'
import { ProtectedRoute } from './components/ProtectedRoute'
import { UserMenu } from './components/UserMenu'

const queryClient = new QueryClient()

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <AuthProvider>
        <Router>
          <Routes>
            {/* Login route */}
            <Route 
              path="/login" 
              element={
                <ProtectedRoute requireAuth={false}>
                  <Login />
                </ProtectedRoute>
              } 
            />
            
            {/* Protected app routes */}
            <Route 
              path="/*" 
              element={
                <ProtectedRoute>
                  <AppLayout />
                </ProtectedRoute>
              } 
            />
          </Routes>
        </Router>
      </AuthProvider>
    </QueryClientProvider>
  )
}

function AppLayout() {
  return (
    <div className="min-h-screen bg-gray-50">
      <header className="bg-white shadow-sm border-b">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="flex justify-between items-center py-6">
            <h1 className="text-2xl font-bold text-gray-900">
              Envoy Control Plane
            </h1>
            <div className="flex items-center space-x-8">
              <nav className="flex space-x-8">
                <NavLink
                  to="/"
                  className={({ isActive }) =>
                    `inline-flex items-center px-1 pt-1 text-sm font-medium ${
                      isActive
                        ? 'text-blue-600 border-b-2 border-blue-600'
                        : 'text-gray-500 hover:text-gray-700'
                    }`
                  }
                >
                  <Home className="h-4 w-4 mr-2" />
                  Dashboard
                </NavLink>
                <NavLink
                  to="/clusters"
                  className={({ isActive }) =>
                    `inline-flex items-center px-1 pt-1 text-sm font-medium ${
                      isActive
                        ? 'text-blue-600 border-b-2 border-blue-600'
                        : 'text-gray-500 hover:text-gray-700'
                    }`
                  }
                >
                  <Database className="h-4 w-4 mr-2" />
                  Clusters
                </NavLink>
                <NavLink
                  to="/routes"
                  className={({ isActive }) =>
                    `inline-flex items-center px-1 pt-1 text-sm font-medium ${
                      isActive
                        ? 'text-blue-600 border-b-2 border-blue-600'
                        : 'text-gray-500 hover:text-gray-700'
                    }`
                  }
                >
                  <RouteIcon className="h-4 w-4 mr-2" />
                  Routes
                </NavLink>
                <NavLink
                  to="/config"
                  className={({ isActive }) =>
                    `inline-flex items-center px-1 pt-1 text-sm font-medium ${
                      isActive
                        ? 'text-blue-600 border-b-2 border-blue-600'
                        : 'text-gray-500 hover:text-gray-700'
                    }`
                  }
                >
                  <Settings className="h-4 w-4 mr-2" />
                  Config
                </NavLink>
              </nav>
              <UserMenu />
            </div>
          </div>
        </div>
      </header>
      <main className="max-w-7xl mx-auto py-6 sm:px-6 lg:px-8">
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/clusters" element={<Clusters />} />
          <Route path="/routes" element={<RoutesPage />} />
          <Route path="/config" element={<ConfigPage />} />
        </Routes>
      </main>
    </div>
  )
}

function ConfigPage() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold text-gray-900">Configuration</h1>
        <p className="mt-2 text-gray-600">
          Generate Envoy configuration files
        </p>
      </div>
      <div className="bg-white shadow rounded-lg p-6">
        <p className="text-gray-500">Configuration generation coming soon...</p>
      </div>
    </div>
  )
}

export default App
