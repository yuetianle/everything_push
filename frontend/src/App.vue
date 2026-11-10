<script setup lang="ts">
import { useRoute } from 'vue-router'
const route = useRoute()
const menu = [
  { path: '/', label: '汇率总览' },
  { path: '/charts', label: '汇率图表' },
  { path: '/rules', label: '告警规则' },
  { path: '/notifications', label: '通知历史' },
  { path: '/deposits', label: '存款管理' },
]
</script>

<template>
  <el-container v-if="route.path !== '/login'" style="height: 100vh">
    <el-aside width="200px">
      <el-menu router :default-active="route.path">
        <el-menu-item v-for="m in menu" :key="m.path" :index="m.path">{{ m.label }}</el-menu-item>
      </el-menu>
    </el-aside>
    <el-main><router-view /></el-main>
  </el-container>
  <router-view v-else />
</template>
