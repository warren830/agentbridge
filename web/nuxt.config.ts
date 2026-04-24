export default defineNuxtConfig({
  compatibilityDate: '2025-05-15',
  modules: ['@pinia/nuxt'],
  ssr: false, // SPA mode — no server-side rendering needed
  devtools: { enabled: false },
  app: {
    head: {
      title: 'AgentPush',
      meta: [
        { charset: 'utf-8' },
        { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      ],
    },
  },
  // Proxy API to gateway during dev
  nitro: {
    devProxy: {
      '/api': { target: 'http://localhost:9900', changeOrigin: true },
    },
  },
})
