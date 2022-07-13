import { render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import { baseNodeAllState } from '../../../__tests__/mocks/states'

import themes from '../../styles/themes'

import { rootReducer } from '../../store'

import BaseNodeQRModal from '.'

describe('BaseNodeQRModal', () => {
  it('should render modal with the QR Code without crashing', () => {
    const onCloseFn = jest.fn()
    const store = configureStore({
      reducer: rootReducer,
      preloadedState: {
        baseNode: baseNodeAllState,
      },
    })

    render(
      <ThemeProvider theme={themes.light}>
        <Provider store={store}>
          <BaseNodeQRModal open onClose={onCloseFn} />
        </Provider>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('base-node-qr-code')
    expect(el).toBeInTheDocument()
  })
})
