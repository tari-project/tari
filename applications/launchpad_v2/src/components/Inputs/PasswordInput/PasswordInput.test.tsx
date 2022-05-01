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
        <PasswordInput value='password for testing' />
      </ThemeProvider>,
    )

    const passwordIcon = screen.getByTestId('icon-test')
    expect(passwordIcon).toBeInTheDocument()

    fireEvent.click(passwordIcon)

    const afterClick = screen.getByDisplayValue('password for testing')
    expect(afterClick.getAttribute('type')).toEqual('text')
  })
})
