<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { apiDel, apiGet, apiPost, apiPut } from '../api/client'

interface Rule {
  id: number; source: string; pair: string; price_type: string; direction: string
  threshold: string; cooldown_minutes: number; enabled: boolean
}

const rows = ref<Rule[]>([])
const pairs = ['USD/CNY', 'EUR/CNY', 'JPY/CNY', 'HKD/CNY', 'GBP/CNY', 'AUD/CNY']
const sources = [
  { v: 'api', label: '市场基准' }, { v: 'boc', label: '中行' },
  { v: 'icbc', label: '工行' }, { v: 'manual', label: '人工' },
]
const priceTypes = [
  { v: 'fx_bid', label: '现汇买入价' }, { v: 'fx_ask', label: '现汇卖出价' }, { v: 'mid', label: '中间价' },
]
const srcLabel = Object.fromEntries(sources.map((s) => [s.v, s.label]))
const ptLabel = Object.fromEntries(priceTypes.map((s) => [s.v, s.label]))

const empty = { source: 'api', pair: 'USD/CNY', price_type: 'mid', direction: 'above', threshold: 7.3, cooldown_minutes: 60, enabled: true }
const dialog = ref(false)
const editId = ref<number | null>(null)
const form = ref({ ...empty })

async function load() {
  rows.value = await apiGet<Rule[]>('/alert-rules')
}
onMounted(load)

function openNew() {
  editId.value = null
  form.value = { ...empty }
  dialog.value = true
}
function openEdit(r: Rule) {
  editId.value = r.id
  form.value = { ...r, threshold: Number(r.threshold) }
  dialog.value = true
}
async function save() {
  try {
    if (editId.value == null) await apiPost('/alert-rules', form.value)
    else await apiPut(`/alert-rules/${editId.value}`, form.value)
    dialog.value = false
    load()
  } catch (e) {
    ElMessage.error((e as Error).message)
  }
}
async function remove(r: Rule) {
  await ElMessageBox.confirm('确认删除该规则？', '删除')
  await apiDel(`/alert-rules/${r.id}`)
  load()
}
</script>

<template>
  <div>
    <el-button type="primary" @click="openNew">新建规则</el-button>
    <el-table :data="rows" border style="margin-top: 12px">
      <el-table-column prop="id" label="ID" width="60" />
      <el-table-column label="来源" width="90">
        <template #default="{ row }">{{ srcLabel[row.source] ?? row.source }}</template>
      </el-table-column>
      <el-table-column prop="pair" label="货币对" width="110" />
      <el-table-column label="价位" width="110">
        <template #default="{ row }">{{ ptLabel[row.price_type] }}</template>
      </el-table-column>
      <el-table-column label="条件" width="80">
        <template #default="{ row }">{{ row.direction === 'above' ? '上破' : '下破' }}</template>
      </el-table-column>
      <el-table-column prop="threshold" label="阈值" width="100" />
      <el-table-column prop="cooldown_minutes" label="冷却(分)" width="90" />
      <el-table-column label="启用" width="80">
        <template #default="{ row }">
          <el-switch v-model="row.enabled" @change="apiPut(`/alert-rules/${row.id}`, row)" />
        </template>
      </el-table-column>
      <el-table-column label="操作">
        <template #default="{ row }">
          <el-button link type="primary" @click="openEdit(row)">编辑</el-button>
          <el-button link type="danger" @click="remove(row)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>

    <el-dialog v-model="dialog" :title="editId == null ? '新建规则' : '编辑规则'">
      <el-form label-width="100px">
        <el-form-item label="数据源">
          <el-select v-model="form.source">
            <el-option v-for="s in sources" :key="s.v" :label="s.label" :value="s.v" />
          </el-select>
        </el-form-item>
        <el-form-item label="货币对">
          <el-select v-model="form.pair">
            <el-option v-for="p in pairs" :key="p" :label="p" :value="p" />
          </el-select>
        </el-form-item>
        <el-form-item label="价位类型">
          <el-select v-model="form.price_type">
            <el-option v-for="p in priceTypes" :key="p.v" :label="p.label" :value="p.v" />
          </el-select>
        </el-form-item>
        <el-form-item label="方向">
          <el-radio-group v-model="form.direction">
            <el-radio-button value="above">上破</el-radio-button>
            <el-radio-button value="below">下破</el-radio-button>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="阈值"><el-input-number v-model="form.threshold" :precision="4" :step="0.01" /></el-form-item>
        <el-form-item label="冷却(分钟)"><el-input-number v-model="form.cooldown_minutes" :min="1" /></el-form-item>
        <el-form-item label="启用"><el-switch v-model="form.enabled" /></el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="dialog = false">取消</el-button>
        <el-button type="primary" @click="save">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>
