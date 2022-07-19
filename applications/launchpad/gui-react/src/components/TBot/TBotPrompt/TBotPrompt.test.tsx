import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import { Provider } from 'react-redux'
import { randomFillSync } from 'crypto'

import { store } from '../../../store'
import themes from '../../../styles/themes'
import TBotPrompt from '.'
import { tauriIPCMock } from '../../../../__tests__/mocks/mockTauriIPC'
import { clearMocks } from '@tauri-apps/api/mocks'

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

describe('TBot', () => {
  it('should render the TBotPrompt component without crashing when set to open', () => {
    tauriIPCMock()

    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <TBotPrompt open={true} />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.getByTestId('tbotprompt-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should not render the component when open prop is false', () => {
    tauriIPCMock()

    render(
      <Provider store={store}>
        <ThemeProvider theme={themes.light}>
          <TBotPrompt open={false} />
        </ThemeProvider>
        ,
      </Provider>,
    )

    const el = screen.queryByTestId('tbotprompt-cmp')
    expect(el).not.toBeInTheDocument()
  })
})
