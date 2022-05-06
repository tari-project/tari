import { render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import MiningBox from '.'

import { rootReducer } from '../../../store'
import { initialState as miningInitialState } from '../../../store/mining/index'
import themes from '../../../styles/themes'
import { MiningNodesStatus } from '../../../store/mining/types'

describe('MiningBox', () => {
  it('should render mining box for the unknown status', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            mining: {
              ...miningInitialState,
              tari: {
                ...miningInitialState.tari,
                status: MiningNodesStatus.UNKNOWN,
              },
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningBox node='tari' />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('node-box-placeholder--unknown')
    expect(el).toBeInTheDocument()
  })

  it('should render mining box for the error status', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            mining: {
              ...miningInitialState,
              tari: {
                ...miningInitialState.tari,
                status: MiningNodesStatus.ERROR,
              },
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningBox node='tari' />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('node-box-placeholder--error')
    expect(el).toBeInTheDocument()
  })

  it('should render mining box for the setup required status', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            mining: {
              ...miningInitialState,
              tari: {
                ...miningInitialState.tari,
                status: MiningNodesStatus.SETUP_REQUIRED,
              },
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningBox node='tari' />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('node-box-placeholder--setup-required')
    expect(el).toBeInTheDocument()
  })

  it('should render mining box for the blocked status', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            mining: {
              ...miningInitialState,
              tari: {
                ...miningInitialState.tari,
                status: MiningNodesStatus.BLOCKED,
              },
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningBox node='tari' />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('node-box-placeholder--blocked')
    expect(el).toBeInTheDocument()
  })

  it('should render mining box for the paused status', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            mining: {
              ...miningInitialState,
              tari: {
                ...miningInitialState.tari,
                status: MiningNodesStatus.PAUSED,
              },
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningBox node='tari' />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('mining-box-paused-content')
    expect(el).toBeInTheDocument()
  })

  it('should render mining box for the running status', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            mining: {
              ...miningInitialState,
              tari: {
                ...miningInitialState.tari,
                status: MiningNodesStatus.RUNNING,
              },
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningBox node='tari' />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('mining-box-running-content')
    expect(el).toBeInTheDocument()
  })

  it('should render mining custom children', () => {
    const testText = 'it is custom test text'
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            mining: {
              ...miningInitialState,
              tari: {
                ...miningInitialState.tari,
                status: MiningNodesStatus.RUNNING,
              },
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningBox node='tari'>
            <span>{testText}</span>
          </MiningBox>
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByText(testText)
    expect(el).toBeInTheDocument()
  })
})
