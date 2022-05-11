import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'

import Alert from './'

describe('Alert', () => {
  it('should call onClose when Close button is clicked', () => {
    const onClose = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <Alert open={true} onClose={onClose} content={<p>alert content</p>} />
      </ThemeProvider>,
    )

    const closeButton = screen.getByText('Close')
    fireEvent.click(closeButton)

    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('should render title when provided', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Alert
          open={true}
          onClose={() => null}
          content={<p>alert content</p>}
          title='alert title'
        />
      </ThemeProvider>,
    )

    const title = screen.getByText('alert title')

    expect(title).toBeInTheDocument()
  })
})
