import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import { Provider } from 'react-redux'

import { store } from '../../../../store'
import themes from '../../../../styles/themes'
import GotItButton from '.'

describe('GotItButton', () => {
  it('should render the button without crashing', () => {
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <GotItButton />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByTestId('gotitbutton-cmp')
    expect(el).toBeInTheDocument()
  })
})
