<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { apiDel, apiGet, apiPost, apiPut } from '../api/client'
import { formatMoney } from '../utils/format'

interface Deposit {
  id: number; person: string; bank: string; currency: string; amount: string; rate: string
  start_date: string; term_months: number; maturity_date: string
  remind_7d: boolean; remind_1d: boolean; remind_today: boolean; note: string | null
}
interface Summary {
  currencies: { currency: string; count: number; total: string; total_maturity: string }[]
  due_30d: Deposit[]
}

const rows = ref<Deposit[]>([])
const summary = ref<Summary | null>(null)
const mask = ref(localStorage.getItem('mask') === '1')

const empty = {
  person: '', bank: '', currency: 'CNY', amount: 10000, rate: 2.5,
  start_date: new Date().toISOString().slice(0, 10), term_months: 12,
  remind_7d: true, remind_1d: true, remind_today: true, note: '',
}
const dialog = ref(false)
const editId = ref<number | null>(null)
const form = ref({ ...empty })

const interestPreview = computed(() => {
  const f = form.value
  return (f.amount * f.rate / 100 * f.term_months / 12).toFixed(2)
})
const maturityPreview = computed(() => {
  const d = new Date(form.value.start_date)
  d.setMonth(d.getMonth() + form.value.term_months)
  return d.toISOString().slice(0, 10)
})

async function load() {
  rows.value = await apiGet<Deposit[]>('/deposits')
  summary.value = await apiGet<Summary>('/deposits/summary')
}
onMounted(load)

function toggleMask() {
  mask.value = !mask.value
  localStorage.setItem('mask', mask.value ? '1' : '0')
}
function openNew() {
  editId.value = null
  form.value = { ...empty }
  dialog.value = true
}
function openEdit(d: Deposit) {
  editId.value = d.id
  form.value = { ...d, amount: Number(d.amount), rate: Number(d.rate), note: d.note ?? '' }
  dialog.value = true
}
async function save() {
  try {
    if (editId.value == null) await apiPost('/deposits', form.value)
    else await apiPut(`/deposits/${editId.value}`, form.value)
    dialog.value = false
    load()
  } catch (e) {
    ElMessage.error((e as Error).message)
  }
}
async function remove(d: Deposit) {
  await ElMessageBox.confirm(`确认删除 ${d.person} 的 ${d.bank} 存款？`, '删除')
  await apiDel(`/deposits/${d.id}`)
  load()
}
</script>

<template>
  <div v-if="summary">
    <el-row :gutter="12" style="margin-bottom: 12px">
      <el-col v-for="c in summary.currencies" :key="c.currency" :span="8">
        <el-card>
          <template #header>
            {{ c.currency }} · {{ c.count }} 笔
            <el-button link style="float: right" @click="toggleMask">
              {{ mask ? '显示金额' : '隐藏金额' }}
            </el-button>
          </template>
          <p>本金合计：{{ formatMoney(c.total, mask) }}</p>
          <p>到期本息：{{ formatMoney(c.total_maturity, mask) }}</p>
        </el-card>
      </el-col>
    </el-row>

    <h3>未来 30 天到期</h3>
    <el-table :data="summary.due_30d" size="small" border>
      <el-table-column prop="maturity_date" label="到期日" width="120" />
      <el-table-column prop="person" label="人员" width="100" />
      <el-table-column prop="bank" label="银行" width="120" />
      <el-table-column label="金额">
        <template #default="{ row }">{{ formatMoney(row.amount, mask) }}</template>
      </el-table-column>
    </el-table>

    <div style="display: flex; justify-content: space-between; margin: 16px 0 12px">
      <h3 style="margin: 0">全部存款</h3>
      <el-button type="primary" @click="openNew">新增存款</el-button>
    </div>
    <el-table :data="rows" border>
      <el-table-column prop="person" label="人员" width="90" />
      <el-table-column prop="bank" label="银行" width="110" />
      <el-table-column prop="currency" label="币种" width="70" />
      <el-table-column label="金额">
        <template #default="{ row }">{{ formatMoney(row.amount, mask) }}</template>
      </el-table-column>
      <el-table-column label="年利率" width="90">
        <template #default="{ row }">{{ Number(row.rate) }}%</template>
      </el-table-column>
      <el-table-column prop="start_date" label="起存日" width="110" />
      <el-table-column prop="term_months" label="期限(月)" width="85" />
      <el-table-column prop="maturity_date" label="到期日" width="110" />
      <el-table-column label="提醒" width="150">
        <template #default="{ row }">
          <el-tag v-if="row.remind_7d" size="small" style="margin-right: 4px">7天</el-tag>
          <el-tag v-if="row.remind_1d" size="small" style="margin-right: 4px">1天</el-tag>
          <el-tag v-if="row.remind_today" size="small">当天</el-tag>
        </template>
      </el-table-column>
      <el-table-column label="操作" width="130">
        <template #default="{ row }">
          <el-button link type="primary" @click="openEdit(row)">编辑</el-button>
          <el-button link type="danger" @click="remove(row)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>

    <el-dialog v-model="dialog" :title="editId == null ? '新增存款' : '编辑存款'">
      <el-form label-width="100px">
        <el-form-item label="人员"><el-input v-model="form.person" /></el-form-item>
        <el-form-item label="银行"><el-input v-model="form.bank" /></el-form-item>
        <el-form-item label="币种">
          <el-select v-model="form.currency" style="width: 120px">
            <el-option v-for="c in ['CNY', 'USD', 'EUR', 'JPY', 'HKD', 'GBP', 'AUD']" :key="c" :label="c" :value="c" />
          </el-select>
        </el-form-item>
        <el-form-item label="金额"><el-input-number v-model="form.amount" :min="0.01" :precision="2" /></el-form-item>
        <el-form-item label="年利率(%)"><el-input-number v-model="form.rate" :min="0" :precision="3" :step="0.1" /></el-form-item>
        <el-form-item label="起存日"><el-date-picker v-model="form.start_date" type="date" value-format="YYYY-MM-DD" /></el-form-item>
        <el-form-item label="期限(月)"><el-input-number v-model="form.term_months" :min="1" /></el-form-item>
        <el-form-item label="到期日"><span>{{ maturityPreview }}</span></el-form-item>
        <el-form-item label="预计利息"><span>{{ interestPreview }}</span></el-form-item>
        <el-form-item label="提醒档位">
          <el-checkbox v-model="form.remind_7d" label="7天" />
          <el-checkbox v-model="form.remind_1d" label="1天" />
          <el-checkbox v-model="form.remind_today" label="当天" />
        </el-form-item>
        <el-form-item label="备注"><el-input v-model="form.note" /></el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="dialog = false">取消</el-button>
        <el-button type="primary" @click="save">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>
