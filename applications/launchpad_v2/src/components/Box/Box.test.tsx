import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'
import Box from '.'

describe('Box', () => {
  it('should render box and children without crash', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Box>
          <p>box children</p>
        </Box>
      </ThemeProvider>,
    )

    const el = screen.getByText('box children')
    expect(el).toBeInTheDocument()
  })
})
