import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, Edit, Server } from 'lucide-react'
import { apiClient } from '../lib/api-client'

interface Endpoint {
  host: string
  port: number
}

interface Cluster {
  name: string
  endpoints: Endpoint[]
  lb_policy?: string
}

export function Clusters() {
  const [isCreateOpen, setIsCreateOpen] = useState(false)
  const [editingCluster, setEditingCluster] = useState<Cluster | null>(null)
  const queryClient = useQueryClient()

  const { data: clusters = [], isLoading } = useQuery({
    queryKey: ['clusters'],
    queryFn: () => apiClient.getClusters(),
  })

  const createMutation = useMutation({
    mutationFn: (cluster: Cluster) => apiClient.createCluster(cluster),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['clusters'] })
      setIsCreateOpen(false)
    },
  })

  const updateMutation = useMutation({
    mutationFn: ({ name, cluster }: { name: string; cluster: Omit<Cluster, 'name'> }) => 
      apiClient.updateCluster(name, cluster),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['clusters'] })
      setEditingCluster(null)
    },
  })

  const deleteMutation = useMutation({
    mutationFn: (name: string) => apiClient.deleteCluster(name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['clusters'] })
    },
  })

  const handleDelete = (cluster: Cluster) => {
    if (confirm(`Are you sure you want to delete cluster "${cluster.name}"?`)) {
      deleteMutation.mutate(cluster.name)
    }
  }

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
          <h1 className="text-3xl font-bold text-gray-900">Clusters</h1>
          <p className="mt-2 text-gray-600">
            Manage your Envoy cluster configurations
          </p>
        </div>
        <button
          onClick={() => setIsCreateOpen(true)}
          className="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md shadow-sm text-white bg-blue-600 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500"
        >
          <Plus className="h-4 w-4 mr-2" />
          Create Cluster
        </button>
      </div>

      {clusters.length === 0 ? (
        <div className="text-center py-12 bg-white rounded-lg shadow">
          <Server className="mx-auto h-12 w-12 text-gray-400" />
          <h3 className="mt-2 text-sm font-medium text-gray-900">No clusters</h3>
          <p className="mt-1 text-sm text-gray-500">
            Get started by creating your first cluster.
          </p>
          <div className="mt-6">
            <button
              onClick={() => setIsCreateOpen(true)}
              className="inline-flex items-center px-4 py-2 border border-transparent shadow-sm text-sm font-medium rounded-md text-white bg-blue-600 hover:bg-blue-700"
            >
              <Plus className="h-4 w-4 mr-2" />
              Create Cluster
            </button>
          </div>
        </div>
      ) : (
        <div className="bg-white shadow overflow-hidden sm:rounded-md">
          <ul className="divide-y divide-gray-200">
            {clusters.map((cluster) => (
              <li key={cluster.name}>
                <div className="px-4 py-4 sm:px-6">
                  <div className="flex items-center justify-between">
                    <div className="flex-1">
                      <p className="text-sm font-medium text-blue-600 truncate">
                        {cluster.name}
                      </p>
                      <div className="mt-2 sm:flex sm:justify-between">
                        <div className="sm:flex">
                          <p className="flex items-center text-sm text-gray-500">
                            <Server className="flex-shrink-0 mr-1.5 h-4 w-4 text-gray-400" />
                            {cluster.endpoints.length} endpoint{cluster.endpoints.length !== 1 ? 's' : ''}
                          </p>
                          <p className="mt-2 flex items-center text-sm text-gray-500 sm:mt-0 sm:ml-6">
                            Load Balancing: {cluster.lb_policy || 'ROUND_ROBIN'}
                          </p>
                        </div>
                      </div>
                      <div className="mt-3">
                        <div className="text-sm text-gray-600">
                          <strong>Endpoints ({cluster.endpoints.length}):</strong>
                        </div>
                        <div className="mt-2 grid grid-cols-1 sm:grid-cols-2 gap-2">
                          {cluster.endpoints.map((endpoint, idx) => (
                            <div 
                              key={idx}
                              className="flex items-center justify-between bg-gray-50 px-3 py-2 rounded-md"
                            >
                              <span className="text-sm font-medium text-gray-800">
                                {endpoint.host}:{endpoint.port}
                              </span>
                              <span className="text-xs text-gray-500">
                                #{idx + 1}
                              </span>
                            </div>
                          ))}
                        </div>
                      </div>
                    </div>
                    <div className="ml-2 flex-shrink-0 flex space-x-2">
                      <button
                        onClick={() => setEditingCluster(cluster)}
                        className="inline-flex items-center p-2 border border-gray-300 rounded-md shadow-sm text-sm font-medium text-gray-700 bg-white hover:bg-gray-50"
                      >
                        <Edit className="h-4 w-4" />
                      </button>
                      <button
                        onClick={() => handleDelete(cluster)}
                        className="inline-flex items-center p-2 border border-red-300 rounded-md shadow-sm text-sm font-medium text-red-700 bg-white hover:bg-red-50"
                        disabled={deleteMutation.isPending}
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                    </div>
                  </div>
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}

      {(isCreateOpen || editingCluster) && (
        <ClusterForm
          cluster={editingCluster}
          onSubmit={(cluster) => {
            if (editingCluster) {
              // Update existing cluster
              updateMutation.mutate({ 
                name: editingCluster.name, 
                cluster: {
                  endpoints: cluster.endpoints,
                  lb_policy: cluster.lb_policy
                }
              })
            } else {
              // Create new cluster
              createMutation.mutate(cluster)
            }
            setEditingCluster(null)
          }}
          onClose={() => {
            setIsCreateOpen(false)
            setEditingCluster(null)
          }}
          isLoading={createMutation.isPending || updateMutation.isPending}
        />
      )}
    </div>
  )
}

interface ClusterFormProps {
  cluster?: Cluster | null
  onSubmit: (cluster: Cluster) => void
  onClose: () => void
  isLoading: boolean
}

// Map between frontend display values and backend API values
const mapToApiPolicy = (frontendPolicy: string): string => {
  const mapping: Record<string, string> = {
    'ROUND_ROBIN': 'ROUND_ROBIN',
    'LEAST_REQUEST': 'LEAST_REQUEST', 
    'RANDOM': 'RANDOM',
    'RING_HASH': 'RING_HASH',
    // Handle backend enum serialization format
    'RoundRobin': 'ROUND_ROBIN',
    'LeastRequest': 'LEAST_REQUEST',
    'Random': 'RANDOM', 
    'RingHash': 'RING_HASH'
  }
  return mapping[frontendPolicy] || frontendPolicy
}

const mapFromApiPolicy = (backendPolicy?: string): string => {
  if (!backendPolicy) return 'ROUND_ROBIN'
  const mapping: Record<string, string> = {
    'ROUND_ROBIN': 'ROUND_ROBIN',
    'LEAST_REQUEST': 'LEAST_REQUEST',
    'RANDOM': 'RANDOM', 
    'RING_HASH': 'RING_HASH',
    // Handle backend enum serialization format
    'RoundRobin': 'ROUND_ROBIN',
    'LeastRequest': 'LEAST_REQUEST',
    'Random': 'RANDOM',
    'RingHash': 'RING_HASH'
  }
  return mapping[backendPolicy] || 'ROUND_ROBIN'
}

function ClusterForm({ cluster, onSubmit, onClose, isLoading }: ClusterFormProps) {
  const [name, setName] = useState(cluster?.name || '')
  const [lbPolicy, setLbPolicy] = useState(mapFromApiPolicy(cluster?.lb_policy))
  const [endpoints, setEndpoints] = useState<Endpoint[]>(
    cluster?.endpoints || [{ host: '', port: 80 }]
  )

  const addEndpoint = () => {
    setEndpoints([...endpoints, { host: '', port: 80 }])
  }

  const removeEndpoint = (index: number) => {
    setEndpoints(endpoints.filter((_, i) => i !== index))
  }

  const updateEndpoint = (index: number, field: keyof Endpoint, value: string | number) => {
    const updated = [...endpoints]
    updated[index] = { ...updated[index], [field]: value }
    setEndpoints(updated)
  }

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return

    const validEndpoints = endpoints.filter(ep => ep.host.trim() && ep.port > 0)
    if (validEndpoints.length === 0) return

    const mappedLbPolicy = mapToApiPolicy(lbPolicy)
    const clusterData = {
      name: name.trim(),
      endpoints: validEndpoints,
      lb_policy: mappedLbPolicy === 'ROUND_ROBIN' ? undefined : mappedLbPolicy,
    }
    
    console.log('Submitting cluster data:', clusterData)
    console.log('Valid endpoints count:', validEndpoints.length)
    console.log('Frontend LB Policy:', lbPolicy)
    console.log('Mapped API LB Policy:', mappedLbPolicy)
    console.log('Original cluster lb_policy:', cluster?.lb_policy)
    
    onSubmit(clusterData)
  }

  return (
    <div className="fixed inset-0 bg-gray-600 bg-opacity-50 overflow-y-auto h-full w-full z-50">
      <div className="relative top-20 mx-auto p-5 border w-11/12 max-w-2xl shadow-lg rounded-md bg-white">
        <div className="mt-3">
          <h3 className="text-lg font-medium text-gray-900 mb-4">
            {cluster ? 'Edit Cluster' : 'Create New Cluster'}
          </h3>
          
          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700">
                Cluster Name
              </label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="mt-1 block w-full border border-gray-300 rounded-md shadow-sm py-2 px-3 focus:outline-none focus:ring-blue-500 focus:border-blue-500"
                placeholder="httpbin-service"
                required
                disabled={!!cluster}
              />
            </div>

            <div>
              <label className="block text-sm font-medium text-gray-700">
                Load Balancing Policy
              </label>
              <select
                value={lbPolicy}
                onChange={(e) => setLbPolicy(e.target.value)}
                className="mt-1 block w-full border border-gray-300 rounded-md shadow-sm py-2 px-3 focus:outline-none focus:ring-blue-500 focus:border-blue-500"
              >
                <option value="ROUND_ROBIN">Round Robin</option>
                <option value="LEAST_REQUEST">Least Request</option>
                <option value="RANDOM">Random</option>
                <option value="RING_HASH">Ring Hash</option>
              </select>
            </div>

            <div>
              <div className="flex justify-between items-center mb-2">
                <label className="block text-sm font-medium text-gray-700">
                  Endpoints
                </label>
                <button
                  type="button"
                  onClick={addEndpoint}
                  className="inline-flex items-center px-3 py-1 border border-transparent text-sm font-medium rounded text-blue-700 bg-blue-100 hover:bg-blue-200"
                >
                  <Plus className="h-4 w-4 mr-1" />
                  Add Endpoint
                </button>
              </div>
              
              <div className="space-y-3">
                {endpoints.map((endpoint, index) => (
                  <div key={index} className="border border-gray-200 rounded-lg p-3 bg-gray-50">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm font-medium text-gray-700">
                        Endpoint #{index + 1}
                      </span>
                      {endpoints.length > 1 && (
                        <button
                          type="button"
                          onClick={() => removeEndpoint(index)}
                          className="p-1 text-red-600 hover:text-red-800 hover:bg-red-50 rounded"
                        >
                          <Trash2 className="h-4 w-4" />
                        </button>
                      )}
                    </div>
                    <div className="flex items-center space-x-2">
                      <div className="flex-1">
                        <label className="block text-xs text-gray-500 mb-1">Host</label>
                        <input
                          type="text"
                          value={endpoint.host}
                          onChange={(e) => updateEndpoint(index, 'host', e.target.value)}
                          className="w-full border border-gray-300 rounded-md shadow-sm py-2 px-3 focus:outline-none focus:ring-blue-500 focus:border-blue-500"
                          placeholder="httpbin.org"
                          required
                        />
                      </div>
                      <div className="w-24">
                        <label className="block text-xs text-gray-500 mb-1">Port</label>
                        <input
                          type="number"
                          value={endpoint.port}
                          onChange={(e) => updateEndpoint(index, 'port', parseInt(e.target.value) || 0)}
                          className="w-full border border-gray-300 rounded-md shadow-sm py-2 px-3 focus:outline-none focus:ring-blue-500 focus:border-blue-500"
                          placeholder="80"
                          min="1"
                          max="65535"
                          required
                        />
                      </div>
                    </div>
                  </div>
                ))}
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
                className="px-4 py-2 border border-transparent rounded-md shadow-sm text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500 disabled:opacity-50"
              >
                {isLoading ? 'Saving...' : cluster ? 'Update Cluster' : 'Create Cluster'}
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  )
}