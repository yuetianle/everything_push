<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { apiGet } from '../api/client'

interface Row {
  id: number; channel: string; title: string; body: string; status: string
  retries: number; error: string | null; created_at: string
}

const rows = ref<Row[]>([])
const total = ref(0)
const page = ref(1)
const channel = ref<string>('')
const status = ref<string>('')

async function load() {
  const p = new URLSearchParams({ page: String(page.value), page_size: '20' })
  if (channel.value) p.set('channel', channel.value)
  if (status.value) p.set('status', status.value)
  const r = await apiGet<{ rows: Row[]; total: number }>(`/notifications?${p}`)
  rows.value = r.rows
  total.value = r.total
}
onMounted(load)
</script>

<template>
  <div>
    <div style="display: flex; gap: 12px; margin-bottom: 12px">
      <el-select v-model="channel" clearable placeholder="渠道" style="width: 140px" @change="page = 1; load()">
        <el-option label="企业微信" value="wecom" />
        <el-option label="Telegram" value="telegram" />
      </el-select>
      <el-select v-model="status" clearable placeholder="状态" style="width: 140px" @change="page = 1; load()">
        <el-option label="已送达" value="sent" />
        <el-option label="失败" value="failed" />
      </el-select>
    </div>
    <el-table :data="rows" border>
      <el-table-column prop="id" label="ID" width="70" />
      <el-table-column label="渠道" width="110">
        <template #default="{ row }">{{ row.channel === 'wecom' ? '企业微信' : 'Telegram' }}</template>
      </el-table-column>
      <el-table-column prop="title" label="标题" width="120" />
      <el-table-column prop="body" label="内容" show-overflow-tooltip />
      <el-table-column label="状态" width="90">
        <template #default="{ row }">
          <el-tag :type="row.status === 'sent' ? 'success' : 'danger'">
            {{ row.status === 'sent' ? '已送达' : '失败' }}
          </el-tag>
        </template>
      </el-table-column>
      <el-table-column prop="retries" label="重试" width="70" />
      <el-table-column label="时间" width="180">
        <template #default="{ row }">{{ new Date(row.created_at).toLocaleString() }}</template>
      </el-table-column>
    </el-table>
    <el-pagination style="margin-top: 12px" layout="prev, pager, next" :total="total"
      :page-size="20" v-model:current-page="page" @current-change="load" />
  </div>
</template>
