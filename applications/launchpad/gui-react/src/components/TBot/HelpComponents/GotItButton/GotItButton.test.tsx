import { render, screen, fireEvent, act } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import { Provider } from 'react-redux'

import { store } from '../../../../store'
import themes from '../../../../styles/themes'
import GotItButton from '.'

const mockDispatch = jest.fn()

describe('GotItButton', () => {
  it('should render the button without crashing', () => {
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <GotItButton onClick={mockDispatch} />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByTestId('gotitbutton-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should call the onClick when button is clicked', async () => {
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <GotItButton onClick={mockDispatch} />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByTestId('gotitbutton-cmp')
    await act(async () => {
      fireEvent.click(el)
    })

    expect(mockDispatch).toBeCalled()
  })
})
