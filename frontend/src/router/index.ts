import { createRouter, createWebHistory } from 'vue-router'

export const routes = [
  { path: '/login', component: () => import('../views/Login.vue') },
  { path: '/', component: () => import('../views/Overview.vue') },
  { path: '/charts', component: () => import('../views/Charts.vue') },
  { path: '/rules', component: () => import('../views/Rules.vue') },
  { path: '/notifications', component: () => import('../views/Notifications.vue') },
  { path: '/deposits', component: () => import('../views/Deposits.vue') },
]

const router = createRouter({ history: createWebHistory(), routes })

router.beforeEach(async (to) => {
  if (to.path === '/login') return true
  const { useAuthStore } = await import('../stores/auth')
  const auth = useAuthStore()
  await auth.ensure()
  return auth.ready || '/login'
})

export default router
