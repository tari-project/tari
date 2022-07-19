import { act, render } from '@testing-library/react'
import { randomFillSync } from 'crypto'
import { clearMocks } from '@tauri-apps/api/mocks'
import { ThemeProvider } from 'styled-components'
import { Provider } from 'react-redux'

import App from './App'

import { tauriIPCMock } from '../__tests__/mocks/mockTauriIPC'

import { store } from './store'
import themes from './styles/themes'

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

test('renders without crashing', async () => {
  tauriIPCMock()
  await act(async () => {
    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <App />
        </ThemeProvider>
      </Provider>,
    )
  })
})
