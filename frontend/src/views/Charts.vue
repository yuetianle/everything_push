<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'
import TrendChart from '../components/TrendChart.vue'
import ManualEntryDialog from '../components/ManualEntryDialog.vue'

const pairs = ['USD/CNY', 'EUR/CNY', 'JPY/CNY', 'HKD/CNY', 'GBP/CNY', 'AUD/CNY']
const sources = ['api', 'boc', 'icbc', 'manual']
const srcName: Record<string, string> = { api: '市场基准', boc: '中行', icbc: '工行', manual: '人工' }

const pair = ref('USD/CNY')
const picked = ref<string[]>(['api', 'boc'])
const range = ref<'24h' | '7d' | '30d' | '1y'>('7d')
const field = ref<'mid' | 'fx_bid' | 'fx_ask'>('mid')
const dialogVisible = ref(false)
const refreshKey = ref(0)

const fromTo = computed(() => {
  const hours = { '24h': 24, '7d': 168, '30d': 720, '1y': 8760 }[range.value]
  return { from: new Date(Date.now() - hours * 3600_000).toISOString(), to: new Date().toISOString() }
})

const timer = window.setInterval(() => (refreshKey.value++), 60_000)
onUnmounted(() => clearInterval(timer))
</script>

<template>
  <div>
    <div style="display: flex; gap: 12px; margin-bottom: 12px; flex-wrap: wrap">
      <el-select v-model="pair" style="width: 130px">
        <el-option v-for="p in pairs" :key="p" :label="p" :value="p" />
      </el-select>
      <el-select v-model="picked" multiple style="width: 280px">
        <el-option v-for="s in sources" :key="s" :label="srcName[s]" :value="s" />
      </el-select>
      <el-radio-group v-model="range">
        <el-radio-button value="24h">24时</el-radio-button>
        <el-radio-button value="7d">7天</el-radio-button>
        <el-radio-button value="30d">30天</el-radio-button>
        <el-radio-button value="1y">1年</el-radio-button>
      </el-radio-group>
      <el-radio-group v-model="field">
        <el-radio-button value="mid">中间价</el-radio-button>
        <el-radio-button value="fx_bid">现汇买入</el-radio-button>
        <el-radio-button value="fx_ask">现汇卖出</el-radio-button>
      </el-radio-group>
      <el-button type="primary" plain @click="dialogVisible = true">人工录入</el-button>
    </div>
    <TrendChart v-if="picked.length" :key="`${pair}-${refreshKey}`" :pair="pair" :sources="picked"
      :from="fromTo.from" :to="fromTo.to" :field="field" height="480px" />
    <el-empty v-else description="请至少选择一个数据源" />
    <ManualEntryDialog v-model:visible="dialogVisible" :pairs="pairs" @saved="refreshKey++" />
  </div>
</template>
