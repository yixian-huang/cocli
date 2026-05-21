// web/src/theme/spike.test.tsx
//
// jsdom limitation: `getComputedStyle(...).width` (and other CSS properties)
// return literal `var(--probe)` strings instead of resolved values, because
// jsdom does not implement CSS variable resolution in computed style.
// See: https://github.com/jsdom/jsdom/issues/2532
//
// As a result we cannot validate the Tailwind 4 `:root[data-theme=…]` var
// override contract inside vitest. The test is kept (skipped) to document the
// contract for future runners (e.g. happy-dom or a real browser).
//
// Manual browser verification (run in devtools on http://localhost:8091):
//   const s=document.createElement('style');
//   s.textContent=':root[data-theme="probe"]{--probe:11px}';
//   document.head.appendChild(s);
//   document.documentElement.setAttribute('data-theme','probe');
//   getComputedStyle(document.documentElement).getPropertyValue('--probe');
//   // → " 11px"
import { describe, it, expect, beforeEach } from 'vitest'
import { render } from '@testing-library/react'

describe('Tailwind 4 [data-theme] var override spike', () => {
  beforeEach(() => {
    document.documentElement.removeAttribute('data-theme')
    document.querySelectorAll('style[data-spike]').forEach((n) => n.remove())
  })

  it.skip('reads CSS vars set via [data-theme] selector', () => {
    const style = document.createElement('style')
    style.setAttribute('data-spike', '')
    style.textContent = `
      :root[data-theme="probe-a"] { --probe: 11px; }
      :root[data-theme="probe-b"] { --probe: 22px; }
    `
    document.head.appendChild(style)

    document.documentElement.setAttribute('data-theme', 'probe-a')
    const { container } = render(<div style={{ width: 'var(--probe)' }} data-testid="probe" />)
    const probeA = getComputedStyle(container.firstChild as Element).width
    expect(probeA).toBe('11px')

    document.documentElement.setAttribute('data-theme', 'probe-b')
    const probeB = getComputedStyle(container.firstChild as Element).width
    expect(probeB).toBe('22px')
  })
})
