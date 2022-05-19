import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import { Provider } from 'react-redux'

import { store } from '../../../store'
import themes from '../../../styles/themes'
import TBotPrompt from '.'

describe('TBot', () => {
  it('should render the TBotPrompt component without crashing when set to open', () => {
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <TBotPrompt open={true} />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByTestId('tbotprompt-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should not render the component when open prop is false', () => {
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <TBotPrompt open={false} />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.queryByTestId('tbotprompt-cmp')
    expect(el).not.toBeInTheDocument()
  })
})
