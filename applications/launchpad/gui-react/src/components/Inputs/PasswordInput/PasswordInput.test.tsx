import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import PasswordInput from '.'

import themes from '../../../styles/themes'

afterEach(cleanup)

describe('PasswordInput', () => {
  it('should render input as password initially', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <PasswordInput value='text for testing' />
      </ThemeProvider>,
    )

    const el = screen.getByDisplayValue('text for testing')
    expect(el.getAttribute('type')).toEqual('password')
  })

  it('should show password after show password icon is clicked', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <PasswordInput value='password for testing' useReveal />
      </ThemeProvider>,
    )

    const passwordIcon = screen.getByTestId('reveal-icon-test')
    expect(passwordIcon).toBeInTheDocument()

    fireEvent.click(passwordIcon)

    const afterClick = screen.getByDisplayValue('password for testing')
    expect(afterClick.getAttribute('type')).toEqual('text')
  })

  it('should render password strength meter for weak passwords', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <PasswordInput value='x' useStrengthMeter />
      </ThemeProvider>,
    )

    const meter = screen.getByTestId('strength-meter')
    expect(meter).toBeInTheDocument()

    expect(Number(meter.getAttribute('data-strength'))).toBeLessThanOrEqual(
      0.25,
    )
  })

  it('should render password strength meter for medium passwords', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <PasswordInput value='passwo' useStrengthMeter />
      </ThemeProvider>,
    )

    const meter = screen.getByTestId('strength-meter')
    expect(meter).toBeInTheDocument()

    expect(Number(meter.getAttribute('data-strength'))).toBeGreaterThanOrEqual(
      0.5,
    )
    expect(Number(meter.getAttribute('data-strength'))).toBeLessThanOrEqual(
      0.75,
    )
  })

  it('should render password strength meter for strong passwords', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <PasswordInput value='Thi5i5$tron9P#$$wor@' useStrengthMeter />
      </ThemeProvider>,
    )

    const meter = screen.getByTestId('strength-meter')
    expect(meter).toBeInTheDocument()

    expect(Number(meter.getAttribute('data-strength'))).toBeGreaterThanOrEqual(
      0.75,
    )
  })

  it('should render empty password strength meter if value is not set', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <PasswordInput useStrengthMeter />
      </ThemeProvider>,
    )

    const meter = screen.getByTestId('strength-meter')
    expect(meter).toBeInTheDocument()

    expect(Number(meter.getAttribute('data-strength'))).toBe(0)
  })
})
