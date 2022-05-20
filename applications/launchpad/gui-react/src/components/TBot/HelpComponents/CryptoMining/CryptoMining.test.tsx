import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import themes from '../../../../styles/themes'
import { Provider } from 'react-redux'
import { store } from '../../../../store'

import { Message1 } from '.'

describe('CryptoMiningMessages', () => {
  it('should render the message component without crashing when set to open', () => {
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <Message1 />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByTestId('message-cmp')
    expect(el).toBeInTheDocument()
  })
})
