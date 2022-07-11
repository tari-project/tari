import { configureStore } from '@reduxjs/toolkit'
import { cleanup, render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { ThemeProvider } from 'styled-components'

import AmountInput from '.'
import { rootReducer } from '../../../store'

import themes from '../../../styles/themes'

afterEach(cleanup)

describe('AmountInput', () => {
  it('should render the AmountInput without crashing', () => {
    const onChange = jest.fn()

    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {},
        })}
      >
        <ThemeProvider theme={themes.light}>
          <AmountInput onChange={onChange} />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByTestId('amount-input-cmp')
    expect(el).toBeInTheDocument()
  })
})
