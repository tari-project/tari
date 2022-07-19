import { render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import MiningHeaderTip from '.'

import themes from '../../../styles/themes'
import t from '../../../locales'
import { rootReducer } from '../../../store'

import {
  initialMining,
  initialWallet,
  miningWithSessions,
  runningWallet,
  tariContainersRunning,
  unlockedWallet,
} from '../../../../__tests__/mocks/states'

describe('MiningHeaderTip', () => {
  it('should render "one step away" when wallet setup is missing', () => {
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
          <MiningHeaderTip />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByText(t.mining.headerTips.oneStepAway)
    expect(el).toBeInTheDocument()
  })

  it('should render "one click away" when mining node status is PAUSED and tokens were not mined yet', () => {
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
          <MiningHeaderTip />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByText(t.mining.headerTips.oneClickAway)
    expect(el).toBeInTheDocument()
  })

  it('should render "continue mining" when mining node status is PAUSED and tokens were already mined', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            wallet: unlockedWallet,
            mining: miningWithSessions,
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningHeaderTip />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByText(t.mining.headerTips.continueMining)
    expect(el).toBeInTheDocument()
  })

  it('should render "running on" when mining node status is RUNNING', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            wallet: runningWallet(),
            mining: miningWithSessions,
            containers: tariContainersRunning,
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningHeaderTip />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByText(t.mining.headerTips.runningOn)
    expect(el).toBeInTheDocument()
  })
})
