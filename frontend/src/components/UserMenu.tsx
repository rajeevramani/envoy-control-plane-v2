import React, { useState, useRef, useEffect } from 'react'
import { User, LogOut, Shield, ChevronDown } from 'lucide-react'
import { useAuth } from '../lib/auth-context'

export function UserMenu() {
  const [isOpen, setIsOpen] = useState(false)
  const { user, logout, isAuthenticated } = useAuth()
  const menuRef = useRef<HTMLDivElement>(null)

  // Close menu when clicking outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setIsOpen(false)
      }
    }

    document.addEventListener('mousedown', handleClickOutside)
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [])

  const handleLogout = async () => {
    setIsOpen(false)
    await logout()
  }

  if (!isAuthenticated || !user) {
    return null
  }

  const isAdmin = user.roles.includes('admin')

  return (
    <div className="relative" ref={menuRef}>
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center space-x-2 text-gray-700 hover:text-gray-900 px-3 py-2 rounded-md text-sm font-medium transition-colors"
      >
        <div className="flex items-center space-x-2">
          <div className="h-8 w-8 bg-blue-100 rounded-full flex items-center justify-center">
            <User className="h-4 w-4 text-blue-600" />
          </div>
          <div className="hidden sm:block text-left">
            <div className="text-sm font-medium text-gray-900">{user.username}</div>
            <div className="text-xs text-gray-500 flex items-center">
              {isAdmin && <Shield className="h-3 w-3 mr-1 text-green-500" />}
              {isAdmin ? 'Administrator' : 'User'}
            </div>
          </div>
          <ChevronDown className={`h-4 w-4 text-gray-400 transition-transform ${isOpen ? 'rotate-180' : ''}`} />
        </div>
      </button>

      {isOpen && (
        <div className="absolute right-0 mt-2 w-64 bg-white rounded-md shadow-lg ring-1 ring-black ring-opacity-5 z-50">
          <div className="py-1">
            {/* User Info */}
            <div className="px-4 py-3 border-b border-gray-100">
              <div className="flex items-center space-x-3">
                <div className="h-10 w-10 bg-blue-100 rounded-full flex items-center justify-center">
                  <User className="h-5 w-5 text-blue-600" />
                </div>
                <div>
                  <div className="font-medium text-gray-900">{user.username}</div>
                  <div className="text-sm text-gray-500">ID: {user.user_id}</div>
                  <div className="flex items-center mt-1">
                    {isAdmin && <Shield className="h-3 w-3 mr-1 text-green-500" />}
                    <span className="text-xs text-gray-500">
                      {user.roles.join(', ')}
                    </span>
                  </div>
                </div>
              </div>
            </div>

            {/* Permissions Info */}
            <div className="px-4 py-2">
              <div className="text-xs font-medium text-gray-500 uppercase tracking-wider mb-2">
                Permissions
              </div>
              <div className="space-y-1">
                <div className="flex items-center text-xs text-gray-600">
                  <div className="h-2 w-2 bg-green-400 rounded-full mr-2"></div>
                  View clusters and routes
                </div>
                <div className="flex items-center text-xs text-gray-600">
                  <div className={`h-2 w-2 rounded-full mr-2 ${isAdmin ? 'bg-green-400' : 'bg-gray-300'}`}></div>
                  Create and modify resources
                  {!isAdmin && <span className="ml-1 text-gray-400">(read-only)</span>}
                </div>
                <div className="flex items-center text-xs text-gray-600">
                  <div className={`h-2 w-2 rounded-full mr-2 ${isAdmin ? 'bg-green-400' : 'bg-gray-300'}`}></div>
                  Generate configurations
                  {!isAdmin && <span className="ml-1 text-gray-400">(admin only)</span>}
                </div>
              </div>
            </div>

            {/* Logout */}
            <div className="border-t border-gray-100">
              <button
                onClick={handleLogout}
                className="flex w-full items-center px-4 py-2 text-sm text-gray-700 hover:bg-gray-100 transition-colors"
              >
                <LogOut className="h-4 w-4 mr-3 text-gray-400" />
                Sign out
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}