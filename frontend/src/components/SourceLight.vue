<script setup lang="ts">
defineProps<{ snapshots: { name: string; is_down: boolean; consecutive_fails: number; last_ok_at: string | null }[] }>()
const label: Record<string, string> = { api: '市场基准', boc: '中行', icbc: '工行' }
</script>

<template>
  <span v-for="s in snapshots" :key="s.name" style="margin-right: 12px; font-size: 13px">
    <el-tag :type="s.is_down ? 'danger' : 'success'" size="small" effect="dark">
      {{ label[s.name] ?? s.name }} {{ s.is_down ? '失效' : '正常' }}
    </el-tag>
  </span>
</template>
