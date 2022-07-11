import { cleanup, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import Textarea from '.'

import themes from '../../../styles/themes'

afterEach(cleanup)

describe('Textarea', () => {
  it('should render the Text Area without crashing', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Textarea />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('textarea-cmp')
    expect(el).toBeInTheDocument()
  })
})
