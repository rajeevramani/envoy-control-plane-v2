// Frontend validation utilities matching backend validation patterns

/**
 * Validation patterns matching backend regex patterns
 */
export const VALIDATION_PATTERNS = {
  // Route names: alphanumeric, underscore, hyphen only (1-100 chars)
  ROUTE_NAME: /^[a-zA-Z0-9_-]+$/,
  
  // Cluster names: alphanumeric, underscore, period, hyphen only (1-50 chars)  
  CLUSTER_NAME: /^[a-zA-Z0-9_.-]+$/,
  
  // Host validation: alphanumeric, period, hyphen (for domains and IPs)
  HOST: /^[a-zA-Z0-9.-]+$/,
  
  // Path validation: starts with /, contains safe URL characters
  PATH: /^\/[a-zA-Z0-9\/_.-]*$/,
  
  // HTTP method validation: standard HTTP verbs only
  HTTP_METHOD: /^(GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS|TRACE|CONNECT)$/,
  
  // Load balancing policy validation
  LB_POLICY: /^(ROUND_ROBIN|LEAST_REQUEST|RANDOM|RING_HASH)$/
}

/**
 * Validation functions with detailed error messages
 */
export const validators = {
  routeName: (name: string): string | null => {
    if (!name || name.length === 0) {
      return 'Route name is required'
    }
    if (name.length > 100) {
      return 'Route name must be 100 characters or less'
    }
    if (!VALIDATION_PATTERNS.ROUTE_NAME.test(name)) {
      return 'Route name can only contain letters, numbers, underscores, and hyphens'
    }
    return null
  },

  clusterName: (name: string): string | null => {
    if (!name || name.length === 0) {
      return 'Cluster name is required'
    }
    if (name.length > 50) {
      return 'Cluster name must be 50 characters or less'  
    }
    if (!VALIDATION_PATTERNS.CLUSTER_NAME.test(name)) {
      return 'Cluster name can only contain letters, numbers, underscores, periods, and hyphens'
    }
    return null
  },

  path: (path: string): string | null => {
    if (!path || path.length === 0) {
      return 'Path is required'
    }
    if (path.length > 200) {
      return 'Path must be 200 characters or less'
    }
    if (path.includes('..') || path.includes('//')) {
      return 'Path contains invalid sequences (path traversal detected)'
    }
    if (!VALIDATION_PATTERNS.PATH.test(path)) {
      return 'Path must start with / and contain only safe URL characters'
    }
    return null
  },

  host: (host: string): string | null => {
    if (!host || host.length === 0) {
      return 'Host is required'
    }
    if (host.length > 255) {
      return 'Host must be 255 characters or less'
    }
    if (!VALIDATION_PATTERNS.HOST.test(host)) {
      return 'Host can only contain letters, numbers, periods, and hyphens'
    }
    return null
  },

  port: (port: number): string | null => {
    if (!port || port < 1 || port > 65535) {
      return 'Port must be between 1 and 65535'
    }
    return null
  },

  httpMethods: (methods: string[]): string | null => {
    if (methods.length === 0) {
      return null // Optional field
    }
    if (methods.length > 10) {
      return 'Too many HTTP methods (maximum 10)'
    }
    for (const method of methods) {
      if (!VALIDATION_PATTERNS.HTTP_METHOD.test(method)) {
        return `Invalid HTTP method: ${method}`
      }
    }
    return null
  },

  prefixRewrite: (prefix: string): string | null => {
    if (!prefix) {
      return null // Optional field
    }
    if (prefix.length > 100) {
      return 'Prefix rewrite must be 100 characters or less'
    }
    return null
  }
}

/**
 * Utility function to validate an entire route object
 */
export interface RouteValidationErrors {
  name?: string
  path?: string  
  cluster_name?: string
  prefix_rewrite?: string
  http_methods?: string
}

export const validateRoute = (route: {
  name: string
  path: string
  cluster_name: string
  prefix_rewrite?: string
  http_methods?: string[]
}): RouteValidationErrors => {
  const errors: RouteValidationErrors = {}
  
  const nameError = validators.routeName(route.name)
  if (nameError) errors.name = nameError
  
  const pathError = validators.path(route.path)
  if (pathError) errors.path = pathError
  
  const clusterError = validators.clusterName(route.cluster_name)
  if (clusterError) errors.cluster_name = clusterError
  
  if (route.prefix_rewrite) {
    const prefixError = validators.prefixRewrite(route.prefix_rewrite)
    if (prefixError) errors.prefix_rewrite = prefixError
  }
  
  if (route.http_methods && route.http_methods.length > 0) {
    const methodsError = validators.httpMethods(route.http_methods)
    if (methodsError) errors.http_methods = methodsError
  }
  
  return errors
}

/**
 * Check if validation errors object has any errors
 */
export const hasValidationErrors = (errors: RouteValidationErrors): boolean => {
  return Object.keys(errors).length > 0
}