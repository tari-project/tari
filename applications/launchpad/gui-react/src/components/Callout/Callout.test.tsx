import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import Callout from '.'
import themes from '../../styles/themes'

describe('Callout', () => {
  it('should render without crashing when children is a string', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Callout>{testText}</Callout>
      </ThemeProvider>,
    )

    const el = screen.getByText(testText)
    expect(el).toBeInTheDocument()
  })

  it('should render without crashing when children is a React Node', () => {
    const testId = 'the-callout-test-id'
    const testText = 'The callout test text'
    const testCmp = <span data-testid={testId}>{testText}</span>
    render(
      <ThemeProvider theme={themes.light}>
        <Callout>{testCmp}</Callout>
      </ThemeProvider>,
    )

    const elText = screen.getByText(testText)
    expect(elText).toBeInTheDocument()

    const elCmp = screen.getByTestId(testId)
    expect(elCmp).toBeInTheDocument()
  })

  it('should render without crashing when inverted prop is used', () => {
    const testText = 'The callout test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Callout inverted={true}>{testText}</Callout>
      </ThemeProvider>,
    )

    const el = screen.getByText(testText)
    expect(el).toBeInTheDocument()
  })
})
