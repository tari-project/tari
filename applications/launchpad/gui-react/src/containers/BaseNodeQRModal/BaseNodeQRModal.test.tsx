import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'

import BaseNodeQRModal from '.'

describe('BaseNodeQRModal', () => {
  it('should render modal with the QR Code without crashing', () => {
    const onCloseFn = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <BaseNodeQRModal open onClose={onCloseFn} />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('base-node-qr-code')
    expect(el).toBeInTheDocument()
  })
})
