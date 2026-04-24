<template>
  <div class="login-page">
    <div class="login-card">
      <h1>🚀 AgentPush</h1>
      <p>Enter your API token to connect</p>
      <form @submit.prevent="handleLogin">
        <input
          v-model="token"
          type="password"
          placeholder="API Token"
          autofocus
          :disabled="loading"
        />
        <button type="submit" :disabled="!token || loading">
          {{ loading ? 'Verifying…' : 'Connect' }}
        </button>
      </form>
      <p v-if="error" class="error">{{ error }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useAuthStore } from '~/stores/auth'

const auth = useAuthStore()
const token = ref('')
const error = ref('')
const loading = ref(false)
const router = useRouter()

if (auth.isAuthenticated) {
  router.push('/')
}

async function handleLogin() {
  if (!token.value || loading.value) return
  error.value = ''
  loading.value = true
  try {
    const res = await fetch('/api/instances', {
      headers: { Authorization: `Bearer ${token.value}` },
    })
    if (res.status === 401 || res.status === 403) {
      error.value = 'Invalid API token'
      return
    }
    if (!res.ok) {
      error.value = `Server error (HTTP ${res.status})`
      return
    }
    auth.login(token.value)
    router.push('/')
  } catch (e: any) {
    error.value = `Can't reach gateway: ${e?.message || e}`
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.login-page {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100vh;
  background: #1a1a2e;
  color: #e0e0e0;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

.login-card {
  background: #16213e;
  padding: 2rem 3rem;
  border-radius: 12px;
  text-align: center;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
}

h1 { margin-bottom: 0.5rem; font-size: 1.8rem; }
p { color: #888; margin-bottom: 1.5rem; }

input {
  width: 100%;
  padding: 0.75rem 1rem;
  border: 1px solid #333;
  border-radius: 8px;
  background: #0f3460;
  color: #fff;
  font-size: 1rem;
  margin-bottom: 1rem;
  outline: none;
}
input:focus { border-color: #53c28b; }

button {
  width: 100%;
  padding: 0.75rem;
  border: none;
  border-radius: 8px;
  background: #53c28b;
  color: #fff;
  font-size: 1rem;
  cursor: pointer;
  font-weight: 600;
}
button:hover { background: #45a876; }
button:disabled { opacity: 0.5; cursor: not-allowed; }

.error { color: #e74c3c; margin-top: 1rem; }
</style>
