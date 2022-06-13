import { render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import MiningViewActions from '.'

import themes from '../../../styles/themes'
import { rootReducer } from '../../../store'
import {
  initialMining,
  initialWallet,
  unlockedWallet,
} from '../../../../__tests__/mocks/states'

describe('MiningViewActions', () => {
  it('should render mining actions without crash', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            wallet: unlockedWallet,
            mining: initialMining,
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningViewActions
            openScheduling={() => null}
            toggleStatistics={() => null}
            openSettings={() => null}
          />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('mining-action-setup-mining-hours')
    expect(el).toBeInTheDocument()
    expect(el).not.toHaveAttribute('disabled')
  })

  it('set up mining hours should be disabled if none of mining nodes can be run', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            wallet: initialWallet,
            mining: initialMining,
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningViewActions
            openScheduling={() => null}
            toggleStatistics={() => null}
            openSettings={() => null}
          />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('mining-action-setup-mining-hours')
    expect(el).toHaveAttribute('disabled')
  })
})
