import { act, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'
import ProgressBar from '.'

import themes from '../../styles/themes'

describe('ProgressBar', () => {
  it('should render given value', async () => {
    await act(async () => {
      render(
        <ThemeProvider theme={themes.light}>
          <ProgressBar value={50} />
        </ThemeProvider>,
      )
    })

    const tipTextEl = screen.getByText('50%')
    expect(tipTextEl).toBeInTheDocument()
  })

  it('should render negative values as positive values', async () => {
    await act(async () => {
      render(
        <ThemeProvider theme={themes.light}>
          <ProgressBar value={-50} />
        </ThemeProvider>,
      )
    })

    const tipTextEl = screen.getByText('50%')
    expect(tipTextEl).toBeInTheDocument()
  })

  it('should render 100% for values greater than 100', async () => {
    await act(async () => {
      render(
        <ThemeProvider theme={themes.light}>
          <ProgressBar value={150} />
        </ThemeProvider>,
      )
    })

    const tipTextEl = screen.getByText('100%')
    expect(tipTextEl).toBeInTheDocument()
  })
})
