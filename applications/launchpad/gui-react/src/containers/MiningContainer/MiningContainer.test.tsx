import { render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import MiningContainer from '.'

import { rootReducer } from '../../store'
import { initialState as miningInitialState } from '../../store/mining/index'
import themes from '../../styles/themes'

describe('MiningContainer', () => {
  it('should render Tari and Merged boxes with header tip and actions', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            mining: {
              ...miningInitialState,
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <MiningContainer />
        </ThemeProvider>
      </Provider>,
    )
    const elTips = screen.getByTestId('mining-header-tip-cmp')
    expect(elTips).toBeInTheDocument()

    const elActions = screen.getByTestId('mining-view-actions-cmp')
    expect(elActions).toBeInTheDocument()

    const elTariBox = screen.getByTestId('tari-mining-box')
    expect(elTariBox).toBeInTheDocument()

    const elMergedBox = screen.getByTestId('merged-mining-box')
    expect(elMergedBox).toBeInTheDocument()
  })
})
