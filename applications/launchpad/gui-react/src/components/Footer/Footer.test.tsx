import { clearMocks } from '@tauri-apps/api/mocks'
import { act, render, screen } from '@testing-library/react'
import { randomFillSync } from 'crypto'

import Footer from '.'
import { tauriIPCMock } from '../../../__tests__/mocks/mockTauriIPC'

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

describe('Footer', () => {
  it('should render without crashing', async () => {
    tauriIPCMock()

    await act(async () => {
      render(<Footer />)
    })

    const el = screen.getByTestId('footer-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should render text for supported OS type', async () => {
    tauriIPCMock({
      os: {
        arch: 'x86_64',
        platform: 'darwin',
        ostype: 'Darwin',
      },
    })

    await act(async () => {
      render(<Footer />)
    })

    const el = screen.getByTestId('terminal-instructions-in-footer')
    expect(el).toBeInTheDocument()
  })

  it('should NOT render any instructions if met unsupported OS type', async () => {
    tauriIPCMock({
      os: {
        arch: 'x86_64',
        platform: 'darwin',
        ostype: 'unsupported',
      },
    })

    await act(async () => {
      render(<Footer />)
    })

    const el = screen.queryByTestId('terminal-instructions-in-footer')
    expect(el).not.toBeInTheDocument()
  })
})
