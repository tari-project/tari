import { render, screen } from '@testing-library/react'

import KeyboardKeys from '.'

describe('KeyboardKeys', () => {
  it('should render given keys', async () => {
    render(<KeyboardKeys keys={['Ctrl', 'R', 'win']} />)

    const ctrlTile = screen.getByText('Ctrl')
    expect(ctrlTile).toBeInTheDocument()

    const rTile = screen.getByText('R')
    expect(rTile).toBeInTheDocument()

    const winTile = screen.getByTestId('svg-winkey')
    expect(winTile).toBeInTheDocument()
  })
})
