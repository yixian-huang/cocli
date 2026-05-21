import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BrandLogo } from './BrandLogo'

describe('<BrandLogo>', () => {
  it('renders "cocli" text', () => {
    render(<BrandLogo />)
    expect(screen.getByText('cocli')).toBeInTheDocument()
  })

  it('respects size prop', () => {
    const { rerender } = render(<BrandLogo size="sm" />)
    expect(screen.getByText('cocli').className).toMatch(/text-/)
    rerender(<BrandLogo size="lg" />)
    expect(screen.getByText('cocli').className).toMatch(/text-/)
  })
})
