import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'
import Loading from '.'

describe('Loading', () => {
  it('should render loading indicator when loading=true', () => {
    const testId = 'loading=true'
    render(
      <ThemeProvider theme={themes.light}>
        <Loading loading={true} testId={testId} />
      </ThemeProvider>,
    )

    const el = screen.getByTestId(testId)
    expect(el).toBeInTheDocument()
  })

  it('should NOT render loading indicator when loading=false', () => {
    const testId = 'loading=false'
    render(
      <ThemeProvider theme={themes.light}>
        <Loading loading={false} testId={testId} />
      </ThemeProvider>,
    )

    const el = screen.queryByTestId(testId)
    expect(el).not.toBeInTheDocument()
  })
})
