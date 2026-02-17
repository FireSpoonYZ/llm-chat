import { createApp } from 'vue'
import { createPinia } from 'pinia'
import 'element-plus/dist/index.css'
import './styles/global.css'
import App from './App.vue'
import router from './router'
import { useAuthStore } from './stores/auth'
import { initI18n } from './i18n'

async function bootstrap() {
  initI18n()
  const app = createApp(App)
  const pinia = createPinia()
  app.use(pinia)

  const auth = useAuthStore()
  await auth.ensureSession()
  app.use(router)

  app.config.errorHandler = (err) => {
    console.error('Unhandled error:', err)
  }

  app.mount('#app')
}

bootstrap()
