import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'

import NodeBox, { NodeBoxContentPlaceholder } from '.'

describe('NodeBox', () => {
  it('should render without crashing', async () => {
    render(
      <ThemeProvider theme={themes.light}>
        <NodeBox
          tag={{
            text: 'Test text',
            type: 'warning',
          }}
        />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('node-box-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should render placeholder without crashing', async () => {
    const testText = 'Test text in placeholder'
    render(
      <ThemeProvider theme={themes.light}>
        <NodeBoxContentPlaceholder>{testText}</NodeBoxContentPlaceholder>
      </ThemeProvider>,
    )

    const el = screen.getByText(testText)
    expect(el).toBeInTheDocument()
  })

  it('should render placeholder without crashing', async () => {
    const testText = 'Test text in placeholder'
    const testCmp = <span>{testText}</span>
    render(
      <ThemeProvider theme={themes.light}>
        <NodeBoxContentPlaceholder>{testCmp}</NodeBoxContentPlaceholder>
      </ThemeProvider>,
    )

    const el = screen.getByText(testText)
    expect(el).toBeInTheDocument()
  })
})
