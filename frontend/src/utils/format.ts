export function formatMoney(v: string | number, mask = false): string {
  if (mask) return '****'
  return new Intl.NumberFormat('zh-CN', { maximumFractionDigits: 2 }).format(Number(v))
}
