import { describe, it, expect } from 'vitest'
import { render } from '@testing-library/react'
import { Card } from '../Card'

describe('Card interactive mode', () => {
  it('default Card has no hover-lift class', () => {
    const { container } = render(<Card data-testid="c">hello</Card>)
    const el = container.firstChild as HTMLElement
    expect(el.className).not.toMatch(/hover:shadow-elev-hover/)
    expect(el.className).not.toMatch(/cursor-pointer/)
  })

  it('Card interactive=true adds hover/transition/cursor classes', () => {
    const { container } = render(<Card interactive data-testid="c">hello</Card>)
    const el = container.firstChild as HTMLElement
    expect(el.className).toMatch(/hover:shadow-elev-hover/)
    expect(el.className).toMatch(/transition-\[/)
    expect(el.className).toMatch(/cursor-pointer/)
  })
})
