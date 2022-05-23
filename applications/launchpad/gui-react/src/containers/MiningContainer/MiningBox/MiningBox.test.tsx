import { render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import MiningBox from '.'

import { rootReducer } from '../../../store'
import themes from '../../../styles/themes'
import { Container } from '../../../store/containers/types'

const emptyNodeState = {
  session: undefined,
}

const pausedContainersState = {
  running: false,
  pending: false,
  error: undefined,
  dependsOn: [],
}

describe('MiningBox', () => {
  it('should render custom children when provided', () => {
    const testText = 'the test text'

    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {},
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningBox
            node='tari'
            nodeState={emptyNodeState}
            containersState={pausedContainersState}
            containersToStopOnPause={[]}
          >
            <p>{testText}</p>
          </MiningBox>
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByText(testText)
    expect(el).toBeInTheDocument()
  })

  it('should render mining box for the error status', () => {
    const containersState = {
      running: true,
      pending: false,
      error: [{ type: Container.Tor, error: 'Something went wrong' }],
      dependsOn: [
        {
          type: Container.Tor,
          id: 'test-tor-container-id',
          running: false,
          pending: false,
          error: 'Something went wrong',
        },
      ],
    }

    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {},
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningBox
            node='tari'
            nodeState={emptyNodeState}
            containersState={containersState}
            containersToStopOnPause={[]}
          />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('node-box-placeholder--error')
    expect(el).toBeInTheDocument()
  })

  it('should render mining box for the paused status', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {},
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningBox
            node='tari'
            nodeState={emptyNodeState}
            containersState={pausedContainersState}
            containersToStopOnPause={[]}
          />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('mining-box-paused-content')
    expect(el).toBeInTheDocument()
  })

  it('should render mining box for the running status', () => {
    const containersState = {
      ...pausedContainersState,
      running: true,
    }

    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {},
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningBox
            node='tari'
            nodeState={emptyNodeState}
            containersState={containersState}
            containersToStopOnPause={[]}
          />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('mining-box-running-content')
    expect(el).toBeInTheDocument()
  })
})
