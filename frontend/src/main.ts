import { createApp } from 'vue'
import { createPinia } from 'pinia'
import ElementPlus from 'element-plus'
import 'element-plus/dist/index.css'
import { use } from 'echarts/core'
import { CanvasRenderer } from 'echarts/renderers'
import { LineChart } from 'echarts/charts'
import { GridComponent, LegendComponent, TooltipComponent } from 'echarts/components'
import App from './App.vue'
import router from './router'

use([CanvasRenderer, LineChart, GridComponent, LegendComponent, TooltipComponent])

createApp(App).use(createPinia()).use(router).use(ElementPlus).mount('#app')
