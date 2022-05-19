import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import themes from '../../../../styles/themes'

import { Message1, Message2 } from '.'

describe('MergedMiningMessages', () => {
  it('should render the first message component without crashing when set to open', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Message1 />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('message1-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should render the second message component without crashing when set to open', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Message2 />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('message2-cmp')
    expect(el).toBeInTheDocument()
  })
})
