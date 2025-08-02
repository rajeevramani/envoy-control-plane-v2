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
  http_methods?: string[]
}

interface ApiResponse<T> {
  success: boolean
  data: T
  message: string
}

// Authentication interfaces
interface LoginRequest {
  username: string
  password: string
}

interface LoginResponse {
  token: string
  user_id: string
  username: string
  expires_in: number
}

interface UserInfo {
  user_id: string
  username: string
  roles: string[]
}

interface AuthHealth {
  authentication_enabled: boolean
  jwt_issuer: string
  jwt_expiry_hours: number
  available_demo_users: string[]
  demo_credentials: Record<string, string>
}

class ApiClient {
  private baseUrl: string
  private tokenKey = 'envoy_control_plane_token'

  constructor() {
    this.baseUrl = import.meta.env.VITE_API_URL || 'http://127.0.0.1:8080'
  }

  // Token management
  setToken(token: string): void {
    localStorage.setItem(this.tokenKey, token)
  }

  getToken(): string | null {
    return localStorage.getItem(this.tokenKey)
  }

  removeToken(): void {
    localStorage.removeItem(this.tokenKey)
  }

  private getAuthHeaders(): Record<string, string> {
    const token = this.getToken()
    return token ? { Authorization: `Bearer ${token}` } : {}
  }

  private async request<T>(endpoint: string, options?: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl}${endpoint}`, {
      headers: {
        'Content-Type': 'application/json',
        ...this.getAuthHeaders(),
        ...options?.headers,
      },
      ...options,
    })
    
    if (!response.ok) {
      // Handle auth errors specifically
      if (response.status === 401) {
        this.removeToken() // Clear invalid token
        throw new Error('Authentication required')
      }
      if (response.status === 403) {
        throw new Error('Access forbidden')
      }
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

  async updateCluster(name: string, cluster: Omit<Cluster, 'name'>): Promise<string> {
    return this.request<string>(`/clusters/${name}`, {
      method: 'PUT',
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

  async updateRoute(id: string, route: Omit<Route, 'id'>): Promise<string> {
    return this.request<string>(`/routes/${id}`, {
      method: 'PUT',
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

  // HTTP Methods
  async getSupportedHttpMethods(): Promise<string[]> {
    return this.request<string[]>('/supported-http-methods')
  }

  // Authentication Methods
  async login(credentials: LoginRequest): Promise<LoginResponse> {
    const response = await fetch(`${this.baseUrl}/auth/login`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(credentials),
    })

    if (!response.ok) {
      if (response.status === 401) {
        throw new Error('Invalid username or password')
      }
      if (response.status === 503) {
        throw new Error('Authentication is disabled')
      }
      throw new Error(`Login failed: ${response.status}`)
    }

    const result: ApiResponse<LoginResponse> = await response.json()
    
    // Store the token
    this.setToken(result.data.token)
    
    return result.data
  }

  async logout(): Promise<void> {
    try {
      await fetch(`${this.baseUrl}/auth/logout`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...this.getAuthHeaders(),
        },
      })
    } finally {
      // Always remove token even if logout request fails
      this.removeToken()
    }
  }

  async getCurrentUser(): Promise<UserInfo> {
    return this.request<UserInfo>('/auth/me')
  }

  async getAuthHealth(): Promise<AuthHealth> {
    return this.request<AuthHealth>('/auth/health')
  }

  // Check if user is authenticated
  isAuthenticated(): boolean {
    return this.getToken() !== null
  }
}

export const apiClient = new ApiClient()