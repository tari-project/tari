import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'

import NodeBox from '.'

describe('NodeBox', () => {
  it('should render without crashing', async () => {
    render(
      <ThemeProvider theme={themes.light}>
        <NodeBox />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('node-box-cmp')
    expect(el).toBeInTheDocument()
  })
})
