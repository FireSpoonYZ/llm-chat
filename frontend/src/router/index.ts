import { createRouter, createWebHistory } from 'vue-router'

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

router.beforeEach((to) => {
  const token = localStorage.getItem('access_token')
  if (to.meta.requiresAuth && !token) {
    return { name: 'login' }
  }
  if ((to.name === 'login' || to.name === 'register') && token) {
    return { name: 'chat' }
  }
})

export default router
