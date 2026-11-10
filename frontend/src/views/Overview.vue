<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { apiGet } from '../api/client'
import TrendChart from '../components/TrendChart.vue'
import SourceLight from '../components/SourceLight.vue'

interface Latest { pair: string; source: string; fx_bid: string | null; fx_ask: string | null; mid: string | null; recorded_at: string }
interface Snapshot { name: string; is_down: boolean; consecutive_fails: number; last_ok_at: string | null }

const latest = ref<Latest[]>([])
const snapshots = ref<Snapshot[]>([])
const pairs = ['USD/CNY', 'EUR/CNY', 'JPY/CNY', 'HKD/CNY', 'GBP/CNY', 'AUD/CNY']
const srcName: Record<string, string> = { api: '基准', boc: '中行', icbc: '工行', manual: '人工' }

async function load() {
  latest.value = await apiGet<Latest[]>('/rates/latest')
  snapshots.value = await apiGet<Snapshot[]>('/sources/status')
}

let timer: number
onMounted(() => {
  load()
  timer = window.setInterval(load, 60_000)
})
onUnmounted(() => clearInterval(timer))

function iso(offsetHours: number) {
  return new Date(Date.now() - offsetHours * 3600_000).toISOString()
}
</script>

<template>
  <div>
    <div style="margin-bottom: 12px"><SourceLight :snapshots="snapshots" /></div>
    <el-table :data="latest" size="small" border>
      <el-table-column prop="pair" label="货币对" width="110" />
      <el-table-column label="来源" width="80">
        <template #default="{ row }">{{ srcName[row.source] ?? row.source }}</template>
      </el-table-column>
      <el-table-column label="现汇买入">
        <template #default="{ row }">{{ row.fx_bid ? Number(row.fx_bid).toFixed(4) : '-' }}</template>
      </el-table-column>
      <el-table-column label="现汇卖出">
        <template #default="{ row }">{{ row.fx_ask ? Number(row.fx_ask).toFixed(4) : '-' }}</template>
      </el-table-column>
      <el-table-column label="中间价">
        <template #default="{ row }">{{ row.mid ? Number(row.mid).toFixed(4) : '-' }}</template>
      </el-table-column>
      <el-table-column prop="recorded_at" label="时间" width="180">
        <template #default="{ row }">{{ new Date(row.recorded_at).toLocaleString() }}</template>
      </el-table-column>
    </el-table>

    <h3>24 小时基准走势</h3>
    <el-row :gutter="12">
      <el-col v-for="p in pairs" :key="p" :span="8" style="margin-bottom: 12px">
        <el-card>
          <template #header>{{ p }}</template>
          <TrendChart :pair="p" :sources="['api']" :from="iso(24)" :to="new Date().toISOString()"
            field="mid" height="140px" />
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>
