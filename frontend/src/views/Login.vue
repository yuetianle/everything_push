<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { apiPost } from '../api/client'
import { useAuthStore } from '../stores/auth'

const password = ref('')
const loading = ref(false)
const router = useRouter()
const auth = useAuthStore()

async function submit() {
  loading.value = true
  try {
    await apiPost('/auth/login', { password: password.value })
    auth.ready = true
    router.push('/')
  } catch (e) {
    ElMessage.error((e as Error).message)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div style="display: flex; justify-content: center; align-items: center; height: 100vh">
    <el-card style="width: 360px">
      <h2>汇率与定存管理</h2>
      <el-form @submit.prevent="submit">
        <el-form-item>
          <el-input v-model="password" type="password" placeholder="管理员密码" show-password @keyup.enter="submit" />
        </el-form-item>
        <el-button type="primary" :loading="loading" style="width: 100%" @click="submit">登录</el-button>
      </el-form>
    </el-card>
  </div>
</template>
