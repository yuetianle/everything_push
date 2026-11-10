import { describe, expect, it } from 'vitest'
import { formatMoney } from './format'

describe('formatMoney', () => {
  it('formats numbers with thousands separators', () => {
    expect(formatMoney('10000')).toBe('10,000')
  })
  it('masks when requested', () => {
    expect(formatMoney(10000, true)).toBe('****')
  })
})
