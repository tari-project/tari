import { render, screen } from '@testing-library/react'

import Logo from '.'

describe('Logo', () => {
  it('should render the signet variant', () => {
    render(<Logo variant='signet' />)

    const svgEl = screen.getByTestId('svg-tarisignet')
    expect(svgEl).toBeInTheDocument()
  })

  it('should render the logo variant', () => {
    render(<Logo variant='logo' />)

    const svgEl = screen.getByTestId('svg-tarilogo')
    expect(svgEl).toBeInTheDocument()
  })

  it('should render the full variant', () => {
    render(<Logo variant='full' />)

    const svgEl = screen.getByTestId('svg-tarilaunchpadlogo')
    expect(svgEl).toBeInTheDocument()
  })

  it('should render the default variant', () => {
    render(<Logo />)

    const svgEl = screen.getByTestId('svg-tarilogo')
    expect(svgEl).toBeInTheDocument()
  })
})
