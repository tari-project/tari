import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { Provider } from 'react-redux'
import { configureStore } from '@reduxjs/toolkit'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'
import { rootReducer } from '../../store'
import { actions as settingsActions } from '../../store/settings'

import SettingsContainer from '.'
import { Settings } from '../../store/settings/types'

describe('MiningSettings', () => {
  it('should set mining settings using Settings', async () => {
    const store = configureStore({
      reducer: rootReducer,
      preloadedState: {
        mining: {
          notifications: [],
          tari: {},
          merged: {
            address: 'initial-address-value',
            threads: 1,
            urls: [],
            useAuth: false,
          },
        },
      },
    })

    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <SettingsContainer />
        </ThemeProvider>
      </Provider>,
    )

    // 1. Open mining settings
    act(() => {
      store.dispatch(settingsActions.open({}))
      store.dispatch(settingsActions.goTo(Settings.Mining))
    })

    expect(screen.getByTestId('settings-modal-container')).toBeInTheDocument()

    // 2. Find Monero address input, check its current value and set new value
    let addressInput = screen.getByTestId('address-input')
    expect(addressInput).toBeInTheDocument()
    expect((addressInput as HTMLInputElement).value).toBe(
      'initial-address-value',
    )

    await act(async () => {
      fireEvent.input(addressInput, { target: { value: 'new-address-value' } })
    })

    addressInput = screen.getByTestId('address-input')
    expect((addressInput as HTMLInputElement).value).toBe('new-address-value')

    // 3. Change threads value
    const threadsInput = screen.getByTestId('mining-merged-threads-input')
    await act(async () => {
      fireEvent.input(threadsInput, { target: { value: 3 } })
    })

    // 4. Add 2 URLs and then remove one
    const addUrlBtn = screen.getByTestId('add-new-monero-url-btn')
    await act(async () => {
      fireEvent.click(addUrlBtn)
      fireEvent.click(addUrlBtn)
    })

    let firstUrl = screen.getByTestId('mining-url-input-0')
    let secondUrl = screen.getByTestId('mining-url-input-1')

    expect(firstUrl).toBeInTheDocument()
    expect(secondUrl).toBeInTheDocument()

    await act(async () => {
      fireEvent.input(firstUrl, {
        target: { value: 'http://new-monero-url-0.co' },
      })
      fireEvent.input(secondUrl, {
        target: { value: 'http://new-monero-url-1.co' },
      })
    })

    firstUrl = screen.getByTestId('mining-url-input-0')
    secondUrl = screen.getByTestId('mining-url-input-1')
    expect((firstUrl as HTMLInputElement).value).toBe(
      'http://new-monero-url-0.co',
    )
    expect((secondUrl as HTMLInputElement).value).toBe(
      'http://new-monero-url-1.co',
    )

    // remove first url
    const removeFirstUrlBtn = screen.getByTestId('mining-url-remove-0')

    await act(async () => {
      fireEvent.click(removeFirstUrlBtn)
    })

    // 5. Submit form
    const submitBtn = screen.getByTestId('settings-submit-btn')
    expect(submitBtn).toBeInTheDocument()

    // React-hook-form's valdiation is async, so may need to wait a bit until button is unlocked
    waitFor(() => {
      expect(submitBtn).not.toBeDisabled()
    })

    await act(async () => {
      fireEvent.click(submitBtn)
    })

    // Wait for the form submit...
    waitFor(() => {
      expect(submitBtn).not.toBeDisabled()
    })

    // 6. Check state
    const storeState = store.getState()

    expect(storeState.mining.merged.address).toBe('new-address-value')
    expect(storeState.mining.merged.threads).toBe(3)
    expect(
      storeState.mining.merged.urls?.find(
        u => u.url === 'http://new-monero-url-0.co',
      )?.url,
    ).not.toBe('http://new-monero-url-0.co')
    expect(
      storeState.mining.merged.urls?.find(
        u => u.url === 'http://new-monero-url-1.co',
      )?.url,
    ).toBe('http://new-monero-url-1.co')
  })
})
