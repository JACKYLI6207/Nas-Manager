import { createApp } from 'vue'
import './styles/global.css'
import App from './App.vue'

const app = createApp(App)
app.config.errorHandler = (err, _instance, info) => {
  console.error('[Nas Manager]', info, err)
}
app.mount('#app')
