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
            content: 'Test text',
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

  it('should render the correct help icon colour', () => {
    const mock = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <NodeBox
          tag={{ type: 'running', content: 'test' }}
          onHelpPromptClick={mock}
        />
      </ThemeProvider>,
    )

    const textColour = themes.light.inverted.secondary

    const el = screen.getByTestId('help-icon-cmp')
    expect(el).toHaveStyle(`color: ${textColour}`)
  })
})
