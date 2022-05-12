import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'
import TBotPrompt from '.'

describe('TBot', () => {
  it('should render the TBotPrompt component without crashing when set to open', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <TBotPrompt open={true} />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('tbotprompt-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should not render the component when open prop is false', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <TBotPrompt open={false} />
      </ThemeProvider>,
    )

    const el = screen.queryByTestId('tbotprompt-cmp')
    expect(el).not.toBeInTheDocument()
  })
})
