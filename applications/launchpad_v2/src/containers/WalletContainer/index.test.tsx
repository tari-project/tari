import { render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import WalletContainer from '.'

import { rootReducer } from '../../store'
import { initialState as walletInitialState } from '../../store/wallet/index'
import themes from '../../styles/themes'

describe('WalletContainer', () => {
  it('should render password box initially if wallet is not unlocked', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            wallet: {
              ...walletInitialState,
              unlocked: false,
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <WalletContainer />
        </ThemeProvider>
      </Provider>,
    )

    const el = screen.getByText('Enter Password')
    expect(el).toBeInTheDocument()
  })

  it('should render Tari Wallet if wallet is unlocked', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            wallet: {
              ...walletInitialState,
              unlocked: true,
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <WalletContainer />
        </ThemeProvider>
      </Provider>,
    )

    const el = screen.getByText('Tari Wallet')
    expect(el).toBeInTheDocument()
  })

  it('should render balance if wallet is unlocked', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            wallet: {
              ...walletInitialState,
              unlocked: true,
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <WalletContainer />
        </ThemeProvider>
      </Provider>,
    )

    const el = screen.getByText('Balance')
    expect(el).toBeInTheDocument()
  })
})
