interface RetryOptions {
  maxAttempts: number
  baseDelay: number
  maxDelay: number
}

const DEFAULT_RETRY_OPTIONS: RetryOptions = {
  maxAttempts: 3,
  baseDelay: 1000, // 1 second
  maxDelay: 10000, // 10 seconds
}

export async function withRetry<T>(
  fn: () => Promise<T>,
  options: Partial<RetryOptions> = {}
): Promise<T> {
  const { maxAttempts, baseDelay, maxDelay } = { ...DEFAULT_RETRY_OPTIONS, ...options }
  
  let lastError: Error = new Error('Max retry attempts reached')
  
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      return await fn()
    } catch (error) {
      lastError = error as Error
      
      // Don't retry on authentication errors
      if (error instanceof Error && 
          (error.message.includes('Authentication required') || 
           error.message.includes('Access forbidden'))) {
        throw error
      }
      
      if (attempt === maxAttempts) {
        break
      }
      
      // Exponential backoff with jitter
      const delay = Math.min(
        baseDelay * Math.pow(2, attempt - 1) + Math.random() * 1000,
        maxDelay
      )
      
      console.log(`Retry attempt ${attempt}/${maxAttempts} failed, retrying in ${delay}ms...`)
      await new Promise(resolve => setTimeout(resolve, delay))
    }
  }
  
  throw lastError
}

export function isRetryableError(error: Error): boolean {
  // Network errors, timeouts, and 5xx server errors are retryable
  return error.message.includes('timeout') ||
         error.message.includes('Network') ||
         error.message.includes('fetch') ||
         error.name === 'AbortError'
}