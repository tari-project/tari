import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import themes from '../../../../styles/themes'

import { Message1 } from '.'

describe('CryptoMiningMessages', () => {
  it('should render the message component without crashing when set to open', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Message1 />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('message-cmp')
    expect(el).toBeInTheDocument()
  })
})
