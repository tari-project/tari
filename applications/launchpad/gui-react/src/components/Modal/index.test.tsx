import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'

import Modal from '.'

describe('Modal', () => {
  it('should not render children when modal is not open', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Modal open={false} onClose={() => null}>
          <p>child element</p>
        </Modal>
      </ThemeProvider>,
    )

    const el = screen.queryByText('child element')
    expect(el).not.toBeInTheDocument()
  })

  it('should render children when modal is not open', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Modal open={true} onClose={() => null}>
          <p>child element</p>
        </Modal>
      </ThemeProvider>,
    )

    const el = screen.queryByText('child element')
    expect(el).toBeInTheDocument()
  })

  it('should close modal on backdrop click', () => {
    const onClose = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <Modal open={true} onClose={onClose}>
          <p>child element</p>
        </Modal>
      </ThemeProvider>,
    )

    const backdrop = screen.getByTestId('modal-backdrop')
    fireEvent.click(backdrop)

    expect(onClose).toHaveBeenCalled()
  })
})
