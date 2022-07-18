import { render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import MiningBox from '.'

import { rootReducer } from '../../../store'
import themes from '../../../styles/themes'

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
          >
            <p>{testText}</p>
          </MiningBox>
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByText(testText)
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
          />
        </ThemeProvider>
      </Provider>,
    )
    const el = screen.getByTestId('mining-box-running-content')
    expect(el).toBeInTheDocument()
  })
})
