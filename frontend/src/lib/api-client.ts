interface Endpoint {
  host: string
  port: number
}

interface Cluster {
  name: string
  endpoints: Endpoint[]
  lb_policy?: string
}

interface Route {
  id: string
  path: string
  cluster_name: string
  prefix_rewrite?: string
}

interface ApiResponse<T> {
  success: boolean
  data: T
  message: string
}

class ApiClient {
  private baseUrl: string

  constructor() {
    this.baseUrl = import.meta.env.VITE_API_URL || 'http://127.0.0.1:8080'
  }

  private async request<T>(endpoint: string, options?: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl}${endpoint}`, {
      headers: {
        'Content-Type': 'application/json',
        ...options?.headers,
      },
      ...options,
    })
    
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`)
    }
    
    const result: ApiResponse<T> = await response.json()
    return result.data
  }

  async getHealth(): Promise<string> {
    const response = await fetch(`${this.baseUrl}/health`)
    return response.text()
  }

  // Cluster Management
  async getClusters(): Promise<Cluster[]> {
    return this.request<Cluster[]>('/clusters')
  }

  async getCluster(name: string): Promise<Cluster> {
    return this.request<Cluster>(`/clusters/${name}`)
  }

  async createCluster(cluster: Omit<Cluster, 'name'> & { name: string }): Promise<string> {
    return this.request<string>('/clusters', {
      method: 'POST',
      body: JSON.stringify(cluster),
    })
  }

  async deleteCluster(name: string): Promise<void> {
    await this.request<void>(`/clusters/${name}`, {
      method: 'DELETE',
    })
  }

  // Route Management
  async getRoutes(): Promise<Route[]> {
    return this.request<Route[]>('/routes')
  }

  async getRoute(id: string): Promise<Route> {
    return this.request<Route>(`/routes/${id}`)
  }

  async createRoute(route: Omit<Route, 'id'>): Promise<string> {
    return this.request<string>('/routes', {
      method: 'POST',
      body: JSON.stringify(route),
    })
  }

  async deleteRoute(id: string): Promise<void> {
    await this.request<void>(`/routes/${id}`, {
      method: 'DELETE',
    })
  }

  // Configuration Generation
  async generateConfig(proxyName: string, proxyPort: number): Promise<string> {
    return this.request<string>('/generate-config', {
      method: 'POST',
      body: JSON.stringify({
        proxy_name: proxyName,
        proxy_port: proxyPort,
      }),
    })
  }

  async generateBootstrap(): Promise<string> {
    return this.request<string>('/generate-bootstrap')
  }
}

export const apiClient = new ApiClient()