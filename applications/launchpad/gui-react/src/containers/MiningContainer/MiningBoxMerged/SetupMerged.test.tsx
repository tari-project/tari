import { act, fireEvent, render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import { rootReducer } from '../../../store'
import { Settings } from '../../../store/settings/types'
import themes from '../../../styles/themes'
import SettingsContainer from '../../SettingsContainer'

import SetupMerged from './SetupMerged'
import { MergedMiningSetupRequired } from '../../../store/mining/types'

describe('SetupMerged', () => {
  it('should render button to open settings and open the settings when monero address is missing', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {
            settings: {
              open: false,
              which: Settings.Mining,
              serviceSettings: {},
            },
          },
        })}
      >
        <ThemeProvider theme={themes.light}>
          <SetupMerged
            mergedSetupRequired={MergedMiningSetupRequired.MissingMoneroAddress}
          />
          <SettingsContainer />
        </ThemeProvider>
      </Provider>,
    )

    let settingsContainer = screen.queryByText('settings-modal-container')
    expect(settingsContainer).not.toBeInTheDocument()

    const btn = screen.getByTestId('setup-merged-open-settings')
    act(() => {
      fireEvent.click(btn)
    })

    settingsContainer = screen.getByTestId('settings-modal-container')
    expect(settingsContainer).toBeInTheDocument()
  })

  it('the action button should be disabled when the wallet address is missing', () => {
    render(
      <Provider
        store={configureStore({
          reducer: rootReducer,
          preloadedState: {},
        })}
      >
        <ThemeProvider theme={themes.light}>
          <SetupMerged
            mergedSetupRequired={MergedMiningSetupRequired.MissingWalletAddress}
          />
          <SettingsContainer />
        </ThemeProvider>
      </Provider>,
    )

    const btn = screen.getByTestId('setup-merged-open-settings')
    expect(btn).toBeDisabled()
  })
})
