import { render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import MiningViewActions from '.'

import themes from '../../../styles/themes'
import { rootReducer } from '../../../store'
import { MiningNodesStatus } from '../../../store/mining/types'
import { initialState as miningInitialState } from '../../../store/mining/index'

describe('MiningViewActions', () => {
  it('should render mining actions without crash', () => {
    const miningState = {
      ...miningInitialState,
      tari: {
        pending: false,
        status: MiningNodesStatus.PAUSED,
        sessions: [
          {
            total: {
              xtr: '1000',
            },
          },
          {
            total: {
              xtr: '2000',
            },
          },
        ],
      },
      merged: {
        pending: false,
        status: MiningNodesStatus.PAUSED,
      },
    }

    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            mining: {
              ...miningState,
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
<<<<<<< HEAD
          <MiningViewActions openScheduling={() => null} />
=======
          <MiningViewActions />
>>>>>>> launchpad_such_wow
        </ThemeProvider>
      </Provider>,
    )

    const el = screen.getByTestId('mining-action-setup-mining-hours')
    expect(el).toBeInTheDocument()
    expect(el).not.toHaveAttribute('disabled')
  })

  it('set up mining hours should be disabled if none of mining nodes can be run', () => {
    const miningState = {
      ...miningInitialState,
      tari: {
        pending: false,
        status: MiningNodesStatus.SETUP_REQUIRED,
        sessions: [
          {
            total: {
              xtr: '1000',
            },
          },
          {
            total: {
              xtr: '2000',
            },
          },
        ],
      },
      merged: {
        pending: false,
        status: MiningNodesStatus.ERROR,
      },
    }

    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            mining: {
              ...miningState,
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningViewActions openScheduling={() => null} />
        </ThemeProvider>
      </Provider>,
    )

    const el = screen.getByTestId('mining-action-setup-mining-hours')
    expect(el).toHaveAttribute('disabled')
  })
})
