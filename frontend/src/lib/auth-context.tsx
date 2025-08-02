import React, { createContext, useContext, useState, useEffect, ReactNode } from 'react'
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

  const login = async (username: string, password: string) => {
    try {
      const loginResponse = await apiClient.login({ username, password })
      
      // Get user info after successful login
      const userInfo = await apiClient.getCurrentUser()
      setUser(userInfo)
    } catch (error) {
      // Re-throw for the login form to handle
      throw error
    }
  }

  const logout = async () => {
    try {
      await apiClient.logout()
    } catch (error) {
      console.error('Logout error:', error)
    } finally {
      setUser(null)
    }
  }

  const checkAuthStatus = async () => {
    setIsLoading(true)
    try {
      if (apiClient.isAuthenticated()) {
        // Try to get current user info to validate token
        const userInfo = await apiClient.getCurrentUser()
        setUser(userInfo)
      } else {
        setUser(null)
      }
    } catch (error) {
      console.error('Auth check failed:', error)
      setUser(null)
      apiClient.removeToken() // Clear invalid token
    } finally {
      setIsLoading(false)
    }
  }

  // Check auth status on mount
  useEffect(() => {
    checkAuthStatus()
  }, [])

  const value: AuthContextType = {
    user,
    isAuthenticated,
    isLoading,
    login,
    logout,
    checkAuthStatus,
  }

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