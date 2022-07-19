import { configureStore } from '@reduxjs/toolkit'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { ThemeProvider } from 'styled-components'

import AmountInput from '.'
import { rootReducer, store } from '../../../store'

import themes from '../../../styles/themes'

afterEach(cleanup)

const onChangeTextMock = jest.fn()

describe('AmountInput', () => {
  it('should render the AmountInput without crashing', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {},
        })}
      >
        <ThemeProvider theme={themes.light}>
          <AmountInput onChange={onChangeTextMock} />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByTestId('amount-input-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should update input value on change', () => {
    const newInput = 123

    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <AmountInput onChange={onChangeTextMock} />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByTestId('amount-input-cmp')

    fireEvent.change(el, { target: { value: newInput } })
    expect(onChangeTextMock).toHaveBeenCalledWith(newInput)
  })

  it('should render error container if error exists', () => {
    const errorMessage = 'test error'
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <AmountInput
            onChange={onChangeTextMock}
            withError
            error={errorMessage}
          />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByText(errorMessage)
    expect(el).toBeInTheDocument()
  })

  it('should render transaction fee if fee is provided', () => {
    const fee = 1.2

    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <AmountInput
            onChange={onChangeTextMock}
            withFee={true}
            fee={fee}
            value={1}
          />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByTestId('fee-help-button')
    expect(el).toBeInTheDocument()
  })
})
