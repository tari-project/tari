import { render, screen } from '@testing-library/react'

import Footer from '.'

describe('Footer', () => {
  it('should render without crash', () => {
    render(<Footer />)

    const el = screen.getByTestId('footer-cmp')
    expect(el).toBeInTheDocument()
  })
})
