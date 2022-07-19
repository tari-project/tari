import { fireEvent, render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'
import { randomFillSync } from 'crypto'
import { clearMocks } from '@tauri-apps/api/mocks'

import { tauriIPCMock } from '../../../__tests__/mocks/mockTauriIPC'

import MiningContainer from '.'

import themes from '../../styles/themes'

import { rootReducer } from '../../store'
import {
  allStopped,
  initialMining,
  unlockedWallet,
} from '../../../__tests__/mocks/states'
import { act } from 'react-dom/test-utils'
import { Container, SystemEventAction } from '../../store/containers/types'

const rootStateTemplate = {
  wallet: unlockedWallet,
  mining: initialMining,
  containers: allStopped,
}

beforeAll(() => {
  window.crypto = {
    // @ts-expect-error: ignore this
    getRandomValues: function (buffer) {
      // @ts-expect-error: ignore this
      return randomFillSync(buffer)
    },
  }
})

afterEach(() => {
  clearMocks()
})

describe('MiningContainer with Redux', () => {
  it('should toggle the Tari mining node status between running and paused when user clicks on start and pause buttons', async () => {
    // 1. Setup
    // 1.a Set up mocks and store
    tauriIPCMock()

    const containers = [
      Container.Tor,
      Container.BaseNode,
      Container.Wallet,
      Container.SHA3Miner,
    ]

    const store = configureStore({
      reducer: rootReducer,
      preloadedState: {
        ...rootStateTemplate,
      },
    })

    // 1.b Render mining container
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <MiningContainer />
        </ThemeProvider>
      </Provider>,
    )

    // 2. MiningContainer should be in 'paused' status and render the 'start' button
    let elRunBtn = screen.getByTestId('tari-run-btn')
    expect(elRunBtn).toBeInTheDocument()

    // 3. Try to run mining...
    fireEvent.click(elRunBtn)

    // 3.1. Start containers
    // In real world, the backend emits the event that then triggers the 'containers/updateStatus'.
    // We need to mock emitting events, so the following code triggers the 'containers/updateStatus' action manually
    // for each container that is needed to run tari mining.
    await act(async () => {
      containers.forEach(c => {
        store.dispatch({
          type: 'containers/updateStatus',
          payload: {
            containerId: `${c}-id`,
            action: SystemEventAction.Start,
          },
        })
        store.dispatch({
          type: 'containers/start/fulfilled',
          payload: {
            id: `${c}-id`,
            unsubscribeStats: () => null,
          },
          meta: {
            arg: {
              container: c,
            },
          },
        })
      })
    })

    // 3.2 Check that the pause button is rendered
    const elPauseBtn = screen.getByTestId('tari-pause-btn')
    expect(elPauseBtn).toBeInTheDocument()

    // 4. Try to stop mining...
    fireEvent.click(elPauseBtn)

    // 4.1 Stop containers:
    await act(async () => {
      containers.forEach(c => {
        store.dispatch({
          type: 'containers/updateStatus',
          payload: {
            containerId: `${c}-id`,
            action: SystemEventAction.Destroy,
          },
        })
      })
    })

    // 4.2 check that the 'start' button is rendered again.
    screen.findByTestId('tari-run-btn')

    elRunBtn = screen.getByTestId('tari-run-btn')
    expect(elRunBtn).toBeInTheDocument()
  })
})

export {}
