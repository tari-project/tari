import { render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import WalletPasswordWizard from '.'

import { rootReducer } from '../../store'
import { initialState as walletInitialState } from '../../store/wallet/index'
import themes from '../../styles/themes'

describe('WalletPasswordWizard', () => {
  it('should render without crashing when custom submit button text is given', () => {
    const testSubmitBtnText = 'The test text of submit button'

    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            wallet: {
              ...walletInitialState,
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <WalletPasswordWizard submitBtnText={testSubmitBtnText} />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByText(testSubmitBtnText)
    expect(el).toBeInTheDocument()
  })
})
