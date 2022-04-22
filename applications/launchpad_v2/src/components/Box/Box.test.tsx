import { render, screen } from '@testing-library/react'

import Box from '.'

describe('Box', () => {
  it('should render box and children without crash', () => {
    render(
      <Box>
        <p>box children</p>
      </Box>,
    )

    const el = screen.getByText('box children')
    expect(el).toBeInTheDocument()
  })
})
