<script setup lang="ts">
import { ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { apiPost } from '../api/client'

const props = defineProps<{ visible: boolean; pairs: string[] }>()
const emit = defineEmits<{ (e: 'update:visible', v: boolean): void; (e: 'saved'): void }>()

const form = ref({ pair: 'USD/CNY', fx_bid: null as number | null, fx_ask: null as number | null, mid: null as number | null, note: '' })

watch(() => props.visible, (v) => { if (v) form.value = { pair: props.pairs[0], fx_bid: null, fx_ask: null, mid: null, note: '' } })

async function submit() {
  const f = form.value
  if (f.fx_bid == null && f.fx_ask == null && f.mid == null) {
    ElMessage.warning('至少填一个价格')
    return
  }
  try {
    await apiPost('/rates/manual', f)
    ElMessage.success('已录入')
    emit('update:visible', false)
    emit('saved')
  } catch (e) {
    ElMessage.error((e as Error).message)
  }
}
</script>

<template>
  <el-dialog :model-value="visible" title="人工录入汇率" @update:model-value="emit('update:visible', $event)">
    <el-form label-width="90px">
      <el-form-item label="货币对">
        <el-select v-model="form.pair">
          <el-option v-for="p in pairs" :key="p" :label="p" :value="p" />
        </el-select>
      </el-form-item>
      <el-form-item label="现汇买入价"><el-input-number v-model="form.fx_bid" :precision="4" :step="0.01" /></el-form-item>
      <el-form-item label="现汇卖出价"><el-input-number v-model="form.fx_ask" :precision="4" :step="0.01" /></el-form-item>
      <el-form-item label="中间价"><el-input-number v-model="form.mid" :precision="4" :step="0.01" /></el-form-item>
      <el-form-item label="备注"><el-input v-model="form.note" /></el-form-item>
    </el-form>
    <template #footer>
      <el-button @click="emit('update:visible', false)">取消</el-button>
      <el-button type="primary" @click="submit">保存</el-button>
    </template>
  </el-dialog>
</template>
