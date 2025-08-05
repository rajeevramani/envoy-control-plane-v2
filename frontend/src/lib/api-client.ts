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
  permissions: Record<string, string[]>
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

  constructor() {
    this.baseUrl = import.meta.env.VITE_API_URL || 'http://localhost:8080'
  }

  // Token management - now handled via httpOnly cookies
  // These methods are kept for backward compatibility but do nothing
  setToken(_token: string): void {
    // No-op - tokens are now stored in httpOnly cookies
    console.log('🍪 Token storage migrated to httpOnly cookies')
  }

  getToken(): string | null {
    // No-op - tokens are now in httpOnly cookies, not accessible to JS
    return null
  }

  removeToken(): void {
    // No-op - logout endpoint clears the cookie
    console.log('🍪 Logout will clear httpOnly cookie')
  }

  private getAuthHeaders(): Record<string, string> {
    // No Authorization header needed - cookies are sent automatically
    return {}
  }

  private async request<T>(endpoint: string, options?: RequestInit): Promise<T> {
    const controller = new AbortController()
    const timeoutId = setTimeout(() => controller.abort(), 10000) // 10s timeout
    
    try {
      const response = await fetch(`${this.baseUrl}${endpoint}`, {
        credentials: 'include', // Essential for httpOnly cookies
        signal: controller.signal,
        headers: {
          'Content-Type': 'application/json',
          'X-Requested-With': 'XMLHttpRequest', // CSRF protection
          ...this.getAuthHeaders(),
          ...options?.headers,
        },
        ...options,
      })
      
      clearTimeout(timeoutId)
      return await this.handleResponse<T>(response)
    } catch (error) {
      clearTimeout(timeoutId)
      if (error instanceof Error && error.name === 'AbortError') {
        throw new Error('Request timeout - please try again')
      }
      throw error
    }
  }

  private async handleResponse<T>(response: Response): Promise<T> {
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
      credentials: 'include', // Essential for receiving cookies
      headers: {
        'Content-Type': 'application/json',
        'X-Requested-With': 'XMLHttpRequest', // CSRF protection
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
    
    // Token is now stored in httpOnly cookie automatically
    console.log('🍪 Login successful - JWT token stored in secure httpOnly cookie')
    
    return result.data
  }

  async logout(): Promise<void> {
    try {
      await fetch(`${this.baseUrl}/auth/logout`, {
        method: 'POST',
        credentials: 'include', // Essential for sending cookies
        headers: {
          'Content-Type': 'application/json',
          'X-Requested-With': 'XMLHttpRequest', // CSRF protection
          ...this.getAuthHeaders(),
        },
      })
      console.log('🍪 Logout successful - httpOnly cookie cleared by server')
    } finally {
      // Cookie is cleared by the server, nothing to do here
      this.removeToken() // No-op for backward compatibility
    }
  }

  async getCurrentUser(): Promise<UserInfo> {
    return this.request<UserInfo>('/auth/me')
  }

  async getAuthHealth(): Promise<AuthHealth> {
    return this.request<AuthHealth>('/auth/health')
  }

  // Check if user is authenticated
  // Since we can't access httpOnly cookies from JS, we'll determine this
  // by successfully calling an authenticated endpoint
  isAuthenticated(): boolean {
    // This is now a placeholder - actual auth check happens via API calls
    // The auth-context will call getCurrentUser() to determine auth status
    return true // Simplified - will be determined by API calls
  }
}

export const apiClient = new ApiClient()