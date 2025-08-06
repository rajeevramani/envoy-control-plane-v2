import { createContext, useContext, useState, useEffect, useCallback, useMemo } from 'react'
import type { ReactNode } from 'react'
import { apiClient } from './api-client'

interface User {
  user_id: string
  username: string
  roles: string[]
  permissions: Record<string, string[]>
}

interface AuthContextType {
  user: User | null
  isAuthenticated: boolean
  isLoading: boolean
  login: (username: string, password: string) => Promise<void>
  logout: () => Promise<void>
  checkAuthStatus: () => Promise<void>
}

const AuthContext = createContext<AuthContextType | undefined>(undefined)

interface AuthProviderProps {
  children: ReactNode
}

export function AuthProvider({ children }: AuthProviderProps) {
  const [user, setUser] = useState<User | null>(null)
  const [isLoading, setIsLoading] = useState(true)

  const isAuthenticated = user !== null

  const login = useCallback(async (username: string, password: string) => {
    try {
      await apiClient.login({ username, password })
      
      // Get user info after successful login
      const userInfo = await apiClient.getCurrentUser()
      setUser(userInfo)
    } catch (error) {
      // Ensure user state is cleared on failed login
      setUser(null)
      // Re-throw for the login form to handle
      throw error
    }
  }, [])

  const logout = useCallback(async () => {
    try {
      await apiClient.logout()
      console.log('🍪 Logout successful - httpOnly cookie cleared')
    } catch (error) {
      console.error('Logout error:', error)
    } finally {
      setUser(null)
    }
  }, [])

  const checkAuthStatus = useCallback(async () => {
    setIsLoading(true)
    try {
      // Since we can't check httpOnly cookies from JS, we try to get user info
      // If this succeeds, we're authenticated; if it fails, we're not
      const userInfo = await apiClient.getCurrentUser()
      setUser(userInfo)
      console.log('🍪 Authentication verified via httpOnly cookie')
    } catch (error) {
      console.log('🍪 No valid authentication cookie found')
      setUser(null)
      // No need to clear token - it's in httpOnly cookie managed by server
    } finally {
      setIsLoading(false)
    }
  }, [])

  // Check auth status on mount with delay to prevent flash
  useEffect(() => {
    // Small delay to prevent flash of login screen on fast networks
    const timer = setTimeout(() => {
      checkAuthStatus()
    }, 100)
    
    return () => clearTimeout(timer)
  }, [checkAuthStatus])

  const value: AuthContextType = useMemo(() => ({
    user,
    isAuthenticated,
    isLoading,
    login,
    logout,
    checkAuthStatus,
  }), [user, isAuthenticated, isLoading, login, logout, checkAuthStatus])

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>
}

export function useAuth() {
  const context = useContext(AuthContext)
  if (context === undefined) {
    throw new Error('useAuth must be used within an AuthProvider')
  }
  return context
}

// Hook to check if user has specific role
export function useHasRole(role: string) {
  const { user } = useAuth()
  return user?.roles?.includes(role) ?? false
}

// Hook to check if user can perform write operations
export function useCanWrite(resource: string = 'routes') {
  const { user } = useAuth()
  return user?.permissions?.[resource]?.includes('write') ?? false
}

export function useCanDelete(resource: string = 'routes') {
  const { user } = useAuth()
  return user?.permissions?.[resource]?.includes('delete') ?? false
}

export function useCanRead(resource: string = 'routes') {
  const { user } = useAuth()
  return user?.permissions?.[resource]?.includes('read') ?? false
}