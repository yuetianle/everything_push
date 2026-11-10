import { defineStore } from 'pinia'
import { api } from '../api/client'

export const useAuthStore = defineStore('auth', {
  state: () => ({ ready: false }),
  actions: {
    async ensure() {
      if (this.ready) return
      try {
        await api('/auth/me')
        this.ready = true
      } catch {
        /* 401 已由 client 跳转 /login */
      }
    },
    logout() {
      this.ready = false
    },
  },
})
