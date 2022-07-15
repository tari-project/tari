import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import Textarea from '.'

import themes from '../../../styles/themes'

afterEach(cleanup)

const onChangeTextMock = jest.fn()

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

  it('should update textarea value on change', () => {
    const newInput = 'test text'

    render(
      <ThemeProvider theme={themes.light}>
        <Textarea onChange={onChangeTextMock} />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('textarea-cmp')

    fireEvent.change(el, { target: { value: newInput } })
    expect(onChangeTextMock).toHaveBeenCalledWith(newInput)
  })

  it('should render error container if error exists', () => {
    const errorMessage = 'test error'
    render(
      <ThemeProvider theme={themes.light}>
        <Textarea onChange={onChangeTextMock} withError error={errorMessage} />
      </ThemeProvider>,
    )

    const el = screen.getByText(errorMessage)
    expect(el).toBeInTheDocument()
  })
})
