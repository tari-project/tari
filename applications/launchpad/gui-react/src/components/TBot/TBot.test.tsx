import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'
import TBot from '.'

describe('TBot', () => {
  it('should render the TBot component without crashing', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <TBot type='base' />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('tbot-cmp')
    expect(el).toBeInTheDocument()
  })
})
