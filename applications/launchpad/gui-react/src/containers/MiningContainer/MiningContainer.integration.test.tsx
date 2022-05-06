import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import MiningContainer from '.'

import { rootReducer } from '../../store'
import { initialState as miningInitialState } from '../../store/mining/index'
import themes from '../../styles/themes'
import { MiningNodesStatus } from '../../store/mining/types'

describe('MiningContainer with Redux', () => {
  it('should toggle the node status between running and paused', async () => {
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
          <MiningContainer />
        </ThemeProvider>
      </Provider>,
    )
    let elRunBtn = screen.getByTestId('tari-run-btn')
    expect(elRunBtn).toBeInTheDocument()

    fireEvent.click(elRunBtn)
    await waitFor(() => screen.findByTestId('tari-pause-btn'), {
      timeout: 5000,
    })

    expect(await screen.findByTestId('tari-pause-btn')).toBeInTheDocument()

    const elPauseBtn = screen.getByTestId('tari-pause-btn')
    fireEvent.click(elPauseBtn)

    await waitFor(() => screen.findByTestId('tari-run-btn'), {
      timeout: 5000,
    })

    elRunBtn = await screen.getByTestId('tari-run-btn')
    expect(elRunBtn).toBeInTheDocument()
  })
})
