import { defineStore } from 'pinia'

export const useAuthStore = defineStore('auth', {
  state: () => ({
    token: localStorage.getItem('agentbridge_token') || '',
  }),
  getters: {
    isAuthenticated: (state) => !!state.token,
  },
  actions: {
    login(token: string) {
      this.token = token
      localStorage.setItem('agentbridge_token', token)
    },
    logout() {
      this.token = ''
      localStorage.removeItem('agentbridge_token')
    },
  },
})
