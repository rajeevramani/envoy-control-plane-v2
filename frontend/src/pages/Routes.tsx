import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, Edit, Route as RouteIcon, Lock } from 'lucide-react'
import { apiClient } from '../lib/api-client'
import { useCanWrite } from '../lib/auth-context'

interface Route {
  id: string
  path: string
  cluster_name: string
  prefix_rewrite?: string
  http_methods?: string[]
}

interface Cluster {
  name: string
  endpoints: Array<{ host: string; port: number }>
  lb_policy?: string
}

export function Routes() {
  const [isCreateOpen, setIsCreateOpen] = useState(false)
  const [editingRoute, setEditingRoute] = useState<Route | null>(null)
  const queryClient = useQueryClient()
  const canWrite = useCanWrite('routes')

  const { data: routes = [], isLoading: routesLoading } = useQuery({
    queryKey: ['routes'],
    queryFn: () => apiClient.getRoutes(),
  })

  const { data: clusters = [], isLoading: clustersLoading } = useQuery({
    queryKey: ['clusters'],
    queryFn: () => apiClient.getClusters(),
  })

  const { data: supportedHttpMethods = [], isLoading: httpMethodsLoading } = useQuery({
    queryKey: ['supported-http-methods'],
    queryFn: () => apiClient.getSupportedHttpMethods(),
  })

  const createMutation = useMutation({
    mutationFn: (route: Omit<Route, 'id'>) => apiClient.createRoute(route),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['routes'] })
      setIsCreateOpen(false)
    },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, route }: { id: string; route: Omit<Route, 'id'> }) => 
      apiClient.updateRoute(id, route),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['routes'] })
      setEditingRoute(null)
    },
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => apiClient.deleteRoute(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['routes'] })
    },
  })

  const handleDelete = (route: Route) => {
    if (confirm(`Are you sure you want to delete route "${route.path}"?`)) {
      deleteMutation.mutate(route.id)
    }
  }

  const isLoading = routesLoading || clustersLoading || httpMethodsLoading

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold text-gray-900">Routes</h1>
          <p className="mt-2 text-gray-600">
            Manage your Envoy route configurations
          </p>
        </div>
        {canWrite ? (
          <button
            onClick={() => setIsCreateOpen(true)}
            className="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md shadow-sm text-white bg-purple-600 hover:bg-purple-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-purple-500"
            disabled={clusters.length === 0}
          >
            <Plus className="h-4 w-4 mr-2" />
            Create Route
          </button>
        ) : (
          <div className="inline-flex items-center px-4 py-2 border border-gray-300 text-sm font-medium rounded-md text-gray-500 bg-gray-100">
            <Lock className="h-4 w-4 mr-2" />
            Create Route (Admin Only)
          </div>
        )}
      </div>

      {clusters.length === 0 && (
        <div className="bg-yellow-50 border border-yellow-200 rounded-md p-4">
          <div className="flex">
            <div className="ml-3">
              <h3 className="text-sm font-medium text-yellow-800">
                No clusters available
              </h3>
              <div className="mt-2 text-sm text-yellow-700">
                <p>You need to create at least one cluster before you can create routes.</p>
              </div>
            </div>
          </div>
        </div>
      )}

      {routes.length === 0 ? (
        <div className="text-center py-12 bg-white rounded-lg shadow">
          <RouteIcon className="mx-auto h-12 w-12 text-gray-400" />
          <h3 className="mt-2 text-sm font-medium text-gray-900">No routes</h3>
          <p className="mt-1 text-sm text-gray-500">
            Get started by creating your first route.
          </p>
          {clusters.length > 0 && (
            <div className="mt-6">
              <button
                onClick={() => setIsCreateOpen(true)}
                className="inline-flex items-center px-4 py-2 border border-transparent shadow-sm text-sm font-medium rounded-md text-white bg-purple-600 hover:bg-purple-700"
              >
                <Plus className="h-4 w-4 mr-2" />
                Create Route
              </button>
            </div>
          )}
        </div>
      ) : (
        <div className="bg-white shadow overflow-hidden sm:rounded-md">
          <ul className="divide-y divide-gray-200">
            {routes.map((route) => {
              const cluster = clusters.find(c => c.name === route.cluster_name)
              return (
                <li key={route.id}>
                  <div className="px-4 py-4 sm:px-6">
                    <div className="flex items-center justify-between">
                      <div className="flex-1">
                        <p className="text-sm font-medium text-purple-600 truncate">
                          {route.path}
                        </p>
                        <div className="mt-2 sm:flex sm:justify-between">
                          <div className="sm:flex">
                            <p className="flex items-center text-sm text-gray-500">
                              <RouteIcon className="flex-shrink-0 mr-1.5 h-4 w-4 text-gray-400" />
                              → {route.cluster_name}
                              {!cluster && (
                                <span className="ml-2 inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-red-100 text-red-800">
                                  Cluster not found
                                </span>
                              )}
                            </p>
                            {route.prefix_rewrite && (
                              <p className="mt-2 flex items-center text-sm text-gray-500 sm:mt-0 sm:ml-6">
                                Rewrite: {route.prefix_rewrite}
                              </p>
                            )}
                            {route.http_methods && route.http_methods.length > 0 && (
                              <p className="mt-2 flex items-center text-sm text-gray-500 sm:mt-0 sm:ml-6">
                                Methods: {route.http_methods.join(', ')}
                              </p>
                            )}
                          </div>
                        </div>
                        {cluster && (
                          <div className="mt-2">
                            <div className="text-sm text-gray-500">
                              <strong>Target endpoints:</strong>
                              <span className="ml-1">
                                {cluster.endpoints.map(ep => `${ep.host}:${ep.port}`).join(', ')}
                              </span>
                            </div>
                          </div>
                        )}
                      </div>
                      <div className="ml-2 flex-shrink-0 flex space-x-2">
                        {canWrite ? (
                          <>
                            <button
                              onClick={() => setEditingRoute(route)}
                              className="inline-flex items-center p-2 border border-gray-300 rounded-md shadow-sm text-sm font-medium text-gray-700 bg-white hover:bg-gray-50"
                            >
                              <Edit className="h-4 w-4" />
                            </button>
                            <button
                              onClick={() => handleDelete(route)}
                              className="inline-flex items-center p-2 border border-red-300 rounded-md shadow-sm text-sm font-medium text-red-700 bg-white hover:bg-red-50"
                              disabled={deleteMutation.isPending}
                            >
                              <Trash2 className="h-4 w-4" />
                            </button>
                          </>
                        ) : (
                          <div className="inline-flex items-center p-2 border border-gray-200 rounded-md text-sm font-medium text-gray-400 bg-gray-50">
                            <Lock className="h-4 w-4" />
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                </li>
              )
            })}
          </ul>
        </div>
      )}

      {(isCreateOpen || editingRoute) && (
        <RouteForm
          route={editingRoute}
          clusters={clusters}
          supportedHttpMethods={supportedHttpMethods}
          onSubmit={(route) => {
            if (editingRoute) {
              updateMutation.mutate({ id: editingRoute.id, route })
            } else {
              createMutation.mutate(route)
              setIsCreateOpen(false)
            }
          }}
          onClose={() => {
            setIsCreateOpen(false)
            setEditingRoute(null)
          }}
          isLoading={createMutation.isPending || updateMutation.isPending}
        />
      )}
    </div>
  )
}

interface RouteFormProps {
  route?: Route | null
  clusters: Cluster[]
  supportedHttpMethods: string[]
  onSubmit: (route: Omit<Route, 'id'>) => void
  onClose: () => void
  isLoading: boolean
}

function RouteForm({ route, clusters, supportedHttpMethods, onSubmit, onClose, isLoading }: RouteFormProps) {
  const [path, setPath] = useState(route?.path || '')
  const [clusterName, setClusterName] = useState(route?.cluster_name || '')
  const [prefixRewrite, setPrefixRewrite] = useState(route?.prefix_rewrite || '')
  const [selectedMethods, setSelectedMethods] = useState<string[]>(route?.http_methods || [])

  const toggleMethod = (method: string) => {
    setSelectedMethods(prev => 
      prev.includes(method) 
        ? prev.filter(m => m !== method)
        : [...prev, method].sort()
    )
  }

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!path.trim() || !clusterName.trim()) return

    onSubmit({
      path: path.trim(),
      cluster_name: clusterName.trim(),
      prefix_rewrite: prefixRewrite.trim() || undefined,
      http_methods: selectedMethods.length > 0 ? selectedMethods : undefined,
    })
  }

  const pathExamples = [
    '/api/v1/',
    '/users/',
    '/products/',
    '/auth/',
    '/health/',
    '/docs/',
  ]

  return (
    <div className="fixed inset-0 bg-gray-600 bg-opacity-50 overflow-y-auto h-full w-full z-50">
      <div className="relative top-20 mx-auto p-5 border w-11/12 max-w-2xl shadow-lg rounded-md bg-white">
        <div className="mt-3">
          <h3 className="text-lg font-medium text-gray-900 mb-4">
            {route ? 'Edit Route' : 'Create New Route'}
          </h3>
          
          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700">
                Path Pattern
              </label>
              <input
                type="text"
                value={path}
                onChange={(e) => setPath(e.target.value)}
                className="mt-1 block w-full border border-gray-300 rounded-md shadow-sm py-2 px-3 focus:outline-none focus:ring-purple-500 focus:border-purple-500"
                placeholder="/api/v1/"
                required
              />
              <p className="mt-1 text-sm text-gray-500">
                Examples: {pathExamples.join(', ')}
              </p>
            </div>

            <div>
              <label className="block text-sm font-medium text-gray-700">
                Target Cluster
              </label>
              <select
                value={clusterName}
                onChange={(e) => setClusterName(e.target.value)}
                className="mt-1 block w-full border border-gray-300 rounded-md shadow-sm py-2 px-3 focus:outline-none focus:ring-purple-500 focus:border-purple-500"
                required
              >
                <option value="">Select a cluster</option>
                {clusters.map((cluster) => (
                  <option key={cluster.name} value={cluster.name}>
                    {cluster.name} ({cluster.endpoints.length} endpoint{cluster.endpoints.length !== 1 ? 's' : ''})
                  </option>
                ))}
              </select>
              {clusterName && (
                <div className="mt-2 text-sm text-gray-500">
                  {(() => {
                    const selectedCluster = clusters.find(c => c.name === clusterName)
                    return selectedCluster ? (
                      <div>
                        <strong>Endpoints:</strong> {selectedCluster.endpoints.map(ep => `${ep.host}:${ep.port}`).join(', ')}
                      </div>
                    ) : null
                  })()}
                </div>
              )}
            </div>

            <div>
              <label className="block text-sm font-medium text-gray-700">
                Prefix Rewrite (Optional)
              </label>
              <input
                type="text"
                value={prefixRewrite}
                onChange={(e) => setPrefixRewrite(e.target.value)}
                className="mt-1 block w-full border border-gray-300 rounded-md shadow-sm py-2 px-3 focus:outline-none focus:ring-purple-500 focus:border-purple-500"
                placeholder="/v1/"
              />
              <p className="mt-1 text-sm text-gray-500">
                Optional: Rewrite the path prefix when forwarding to the cluster
              </p>
            </div>

            <div>
              <label className="block text-sm font-medium text-gray-700 mb-2">
                HTTP Methods (Optional)
              </label>
              <div className="grid grid-cols-3 gap-2">
                {supportedHttpMethods.map((method) => (
                  <label key={method} className="flex items-center">
                    <input
                      type="checkbox"
                      checked={selectedMethods.includes(method)}
                      onChange={() => toggleMethod(method)}
                      className="h-4 w-4 text-purple-600 focus:ring-purple-500 border-gray-300 rounded"
                    />
                    <span className="ml-2 text-sm text-gray-700">{method}</span>
                  </label>
                ))}
              </div>
              <p className="mt-2 text-sm text-gray-500">
                {selectedMethods.length === 0 
                  ? "No methods selected - route will accept all HTTP methods"
                  : `Selected: ${selectedMethods.join(', ')}`
                }
              </p>
            </div>

            <div className="bg-blue-50 border border-blue-200 rounded-md p-4">
              <h4 className="text-sm font-medium text-blue-800 mb-2">Route Preview</h4>
              <div className="text-sm text-blue-700">
                {path && clusterName ? (
                  <div>
                    <div>
                      {selectedMethods.length > 0 ? (
                        <span><code className="bg-blue-100 px-1 rounded">{selectedMethods.join(', ')}</code> requests</span>
                      ) : (
                        <span>All HTTP requests</span>
                      )}
                      {' '}to <code className="bg-blue-100 px-1 rounded">{path}*</code> will be forwarded to cluster{' '}
                      <code className="bg-blue-100 px-1 rounded">{clusterName}</code>
                      {prefixRewrite && (
                        <span> with path rewritten to <code className="bg-blue-100 px-1 rounded">{prefixRewrite}*</code></span>
                      )}
                    </div>
                  </div>
                ) : (
                  <div className="text-gray-500">Fill in the form to see route preview</div>
                )}
              </div>
            </div>

            <div className="flex justify-end space-x-3 pt-4">
              <button
                type="button"
                onClick={onClose}
                className="px-4 py-2 border border-gray-300 rounded-md shadow-sm text-sm font-medium text-gray-700 bg-white hover:bg-gray-50"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={isLoading}
                className="px-4 py-2 border border-transparent rounded-md shadow-sm text-sm font-medium text-white bg-purple-600 hover:bg-purple-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-purple-500 disabled:opacity-50"
              >
                {isLoading ? 'Saving...' : route ? 'Update Route' : 'Create Route'}
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  )
}