import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import Text from './'

import themes from '../../styles/themes'

describe('Text', () => {
  it('should render Text component without crashing', () => {
    const testText = 'The test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Text>{testText}</Text>
      </ThemeProvider>,
    )

    const el = screen.getByText(testText)
    expect(el).toBeInTheDocument()
  })

  it('should render DOM element of the given type', () => {
    const testText = 'The test text'
    const elType = 'span'
    render(
      <ThemeProvider theme={themes.light}>
        <Text as={elType}>{testText}</Text>
      </ThemeProvider>,
    )

    const el = screen.getByTestId('text-cmp')
    expect(el.tagName.toLowerCase()).toBe(elType)
  })
})
