<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import VChart from 'vue-echarts'
import { apiGet } from '../api/client'

const props = withDefaults(defineProps<{
  pair: string; sources: string[]; from: string; to: string
  field?: 'mid' | 'fx_bid' | 'fx_ask'; height?: string
}>(), { field: 'mid', height: '360px' })

interface Bucket { source: string; t: string; mid: string | null; fx_bid: string | null; fx_ask: string | null }

const series = ref<Bucket[]>([])

async function load() {
  const q = `pair=${props.pair}&sources=${props.sources.join(',')}&from=${props.from}&to=${props.to}&points=300`
  const r = await apiGet<{ points: Bucket[] }>(`/rates/history?${q}`)
  series.value = r.points
}

watch(() => [props.pair, props.sources.join(','), props.from, props.to, props.field], load, { immediate: true })

const option = computed(() => {
  const bySource = new Map<string, [string, number][]>()
  for (const p of series.value) {
    const v = p[props.field]
    if (v != null) {
      const arr = bySource.get(p.source) ?? []
      arr.push([p.t, Number(v)])
      bySource.set(p.source, arr)
    }
  }
  const nameMap: Record<string, string> = { api: '市场基准', boc: '中行', icbc: '工行', manual: '人工' }
  return {
    tooltip: { trigger: 'axis' },
    legend: {},
    xAxis: { type: 'time' },
    yAxis: { type: 'value', scale: true },
    series: [...bySource.entries()].map(([src, data]) => ({
      name: nameMap[src] ?? src, type: 'line', showSymbol: false, data,
    })),
  }
})
</script>

<template>
  <div>
    <VChart :option="option" :style="{ height, width: '100%' }" autoresize />
  </div>
</template>
