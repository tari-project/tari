import { cleanup, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import TextInput from '.'

import themes from '../../../styles/themes'

afterEach(cleanup)

describe('TextInput', () => {
  it('should hide the input text when hideText prop is set to true', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <TextInput hideText value='text for testing' />
      </ThemeProvider>,
    )

    const el = screen.queryByText('text for testing')
    expect(el).not.toBeInTheDocument()
  })
})
