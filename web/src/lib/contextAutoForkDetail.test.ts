import { describe, expect, it } from 'vitest'
import { contextAutoForkModeVariant, parseContextAutoForkDetail } from './contextAutoForkDetail'

describe('parseContextAutoForkDetail', () => {
  it('parses native suffix', () => {
    expect(parseContextAutoForkDetail('context_auto_fork_triggered (native)')).toEqual({
      text: 'context_auto_fork_triggered',
      mode: 'native',
    })
  })

  it('parses restart fallback suffix case-insensitively', () => {
    expect(parseContextAutoForkDetail('context_auto_fork_completed (RESTART FALLBACK)')).toEqual({
      text: 'context_auto_fork_completed',
      mode: 'restart fallback',
    })
  })

  it('keeps non-matching details untouched', () => {
    expect(parseContextAutoForkDetail('context_auto_fork_triggered (91%)')).toEqual({
      text: 'context_auto_fork_triggered (91%)',
      mode: null,
    })
  })
})

describe('contextAutoForkModeVariant', () => {
  it('maps native to success and fallback to warning', () => {
    expect(contextAutoForkModeVariant('native')).toBe('success')
    expect(contextAutoForkModeVariant('restart fallback')).toBe('warning')
  })
})
