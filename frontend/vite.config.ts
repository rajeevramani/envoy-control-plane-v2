import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    host: '127.0.0.1',
    port: 5173,
    // Configure proxy for API calls to handle CORS and cookies
    proxy: {
      '/auth': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        secure: false,
        // Preserve cookies for httpOnly authentication
        configure: (proxy, _options) => {
          proxy.on('proxyReq', (proxyReq, req, _res) => {
            // Ensure credentials are forwarded
            if (req.headers.cookie) {
              proxyReq.setHeader('Cookie', req.headers.cookie)
            }
            proxyReq.setHeader('X-Requested-With', 'XMLHttpRequest')
          })
          proxy.on('proxyRes', (proxyRes, _req, _res) => {
            // Preserve Set-Cookie headers from backend
            if (proxyRes.headers['set-cookie']) {
              proxyRes.headers['set-cookie'] = proxyRes.headers['set-cookie'].map((cookie: string) => {
                // Ensure cookies work in development (same-site handling)
                return cookie.replace(/; SameSite=None/g, '; SameSite=Lax')
              })
            }
          })
        }
      },
      // Proxy all other API calls
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        secure: false,
        rewrite: (path) => path.replace(/^\/api/, ''), // Remove /api prefix
        configure: (proxy, _options) => {
          proxy.on('proxyReq', (proxyReq, req, _res) => {
            if (req.headers.cookie) {
              proxyReq.setHeader('Cookie', req.headers.cookie)
            }
            proxyReq.setHeader('X-Requested-With', 'XMLHttpRequest')
          })
        }
      }
    }
  },
  // Build configuration
  build: {
    outDir: 'dist',
    sourcemap: true,
    // Optimize for production
    rollupOptions: {
      output: {
        manualChunks: {
          vendor: ['react', 'react-dom'],
          router: ['react-router-dom']
        }
      }
    }
  }
})
