import { useQuery } from '@tanstack/react-query'
import { Activity, Database, Route } from 'lucide-react'
import { apiClient } from '../lib/api-client'

export function Dashboard() {
  const { data: clusters, isLoading: clustersLoading } = useQuery({
    queryKey: ['clusters'],
    queryFn: () => apiClient.getClusters(),
  })

  const { data: routes, isLoading: routesLoading } = useQuery({
    queryKey: ['routes'],
    queryFn: () => apiClient.getRoutes(),
  })

  const { data: health } = useQuery({
    queryKey: ['health'],
    queryFn: () => apiClient.getHealth(),
    refetchInterval: 10000,
  })

  const isLoading = clustersLoading || routesLoading

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
      </div>
    )
  }

  const clusterCount = clusters?.length || 0
  const routeCount = routes?.length || 0
  const totalEndpoints = clusters?.reduce((sum, cluster) => sum + cluster.endpoints.length, 0) || 0

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold text-gray-900">Dashboard</h1>
        <p className="mt-2 text-gray-600">
          Monitor your Envoy Control Plane status and configuration
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        <div className="bg-white overflow-hidden shadow rounded-lg">
          <div className="p-5">
            <div className="flex items-center">
              <div className="flex-shrink-0">
                <Database className="h-6 w-6 text-blue-400" />
              </div>
              <div className="ml-5 w-0 flex-1">
                <dl>
                  <dt className="text-sm font-medium text-gray-500 truncate">
                    Total Clusters
                  </dt>
                  <dd className="text-lg font-medium text-gray-900">
                    {clusterCount}
                  </dd>
                </dl>
              </div>
            </div>
          </div>
        </div>

        <div className="bg-white overflow-hidden shadow rounded-lg">
          <div className="p-5">
            <div className="flex items-center">
              <div className="flex-shrink-0">
                <Route className="h-6 w-6 text-purple-400" />
              </div>
              <div className="ml-5 w-0 flex-1">
                <dl>
                  <dt className="text-sm font-medium text-gray-500 truncate">
                    Total Routes
                  </dt>
                  <dd className="text-lg font-medium text-gray-900">
                    {routeCount}
                  </dd>
                </dl>
              </div>
            </div>
          </div>
        </div>

        <div className="bg-white overflow-hidden shadow rounded-lg">
          <div className="p-5">
            <div className="flex items-center">
              <div className="flex-shrink-0">
                <Activity className="h-6 w-6 text-green-400" />
              </div>
              <div className="ml-5 w-0 flex-1">
                <dl>
                  <dt className="text-sm font-medium text-gray-500 truncate">
                    Control Plane Status
                  </dt>
                  <dd className={`text-lg font-medium ${health === 'OK' ? 'text-green-600' : 'text-red-600'}`}>
                    {health === 'OK' ? 'Running' : 'Error'}
                  </dd>
                </dl>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {clusters && clusters.length > 0 && (
          <div className="bg-white shadow rounded-lg">
            <div className="px-4 py-5 sm:p-6">
              <h3 className="text-lg leading-6 font-medium text-gray-900 mb-4">
                Recent Clusters
              </h3>
              <div className="space-y-3">
                {clusters.slice(0, 5).map((cluster) => (
                  <div key={cluster.name} className="flex items-center justify-between py-2 border-b border-gray-100">
                    <div>
                      <p className="text-sm font-medium text-gray-900">{cluster.name}</p>
                      <p className="text-sm text-gray-500">
                        {cluster.endpoints.length} endpoint{cluster.endpoints.length !== 1 ? 's' : ''} • {cluster.lb_policy || 'ROUND_ROBIN'}
                      </p>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}

        {routes && routes.length > 0 && (
          <div className="bg-white shadow rounded-lg">
            <div className="px-4 py-5 sm:p-6">
              <h3 className="text-lg leading-6 font-medium text-gray-900 mb-4">
                Recent Routes
              </h3>
              <div className="space-y-3">
                {routes.slice(0, 5).map((route) => (
                  <div key={route.id} className="flex items-center justify-between py-2 border-b border-gray-100">
                    <div>
                      <p className="text-sm font-medium text-gray-900">{route.path}</p>
                      <p className="text-sm text-gray-500">
                        → {route.cluster_name}
                      </p>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}
      </div>

      {clusterCount === 0 && routeCount === 0 && (
        <div className="text-center py-12">
          <Database className="mx-auto h-12 w-12 text-gray-400" />
          <h3 className="mt-2 text-sm font-medium text-gray-900">No configuration yet</h3>
          <p className="mt-1 text-sm text-gray-500">
            Get started by creating your first cluster and routes.
          </p>
        </div>
      )}
    </div>
  )
}