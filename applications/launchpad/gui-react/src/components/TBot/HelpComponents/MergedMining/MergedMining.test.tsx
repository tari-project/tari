import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import { Provider } from 'react-redux'

import themes from '../../../../styles/themes'
import { store } from '../../../../store'

import { Message2 } from '.'

describe('MergedMiningMessages', () => {
  it('should render the second message component without crashing when set to open', () => {
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <Message2 />
        </ThemeProvider>
      </Provider>,
    )

    const el = screen.getByTestId('message2-cmp')
    expect(el).toBeInTheDocument()
  })
})
