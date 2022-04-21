import React from 'react'
import { render } from '@testing-library/react'
import { randomFillSync } from 'crypto'
import { clearMocks } from '@tauri-apps/api/mocks'
import { ThemeProvider } from 'styled-components'

import App from './App'
import { Provider } from 'react-redux'

import { store } from './store'
import themes from './styles/themes'

beforeAll(() => {
  window.crypto = {
    getRandomValues: function (buffer) {
      return randomFillSync(buffer)
    },
  }
})

afterEach(() => {
  clearMocks()
})

test('renders without crashing', async () => {
  render(
    <Provider store={store}>
      <ThemeProvider theme={themes.light}>
        <App />
      </ThemeProvider>
    </Provider>,
  )
})
