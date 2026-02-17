import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from '../stores/auth'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/login', name: 'login', component: () => import('../views/Login.vue') },
    { path: '/register', name: 'register', component: () => import('../views/Register.vue') },
    {
      path: '/',
      name: 'chat',
      component: () => import('../views/Chat.vue'),
      meta: { requiresAuth: true },
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('../views/Settings.vue'),
      meta: { requiresAuth: true },
    },
    {
      path: '/share/:shareToken',
      name: 'shared-chat',
      component: () => import('../views/SharedChat.vue'),
      props: true,
    },
  ],
})

router.beforeEach(async (to) => {
  const auth = useAuthStore()
  const requiresAuth = !!to.meta.requiresAuth
  const isAuthPage = to.name === 'login' || to.name === 'register'
  if (requiresAuth || isAuthPage) {
    await auth.ensureSession()
  }

  if (requiresAuth && !auth.isAuthenticated) {
    return { name: 'login' }
  }
  if (isAuthPage && auth.isAuthenticated) {
    return { name: 'chat' }
  }
})

export default router
