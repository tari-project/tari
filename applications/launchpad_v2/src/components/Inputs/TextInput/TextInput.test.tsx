import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import TextInput from '.'
import SvgCopy from '../../../styles/Icons/Copy'

import themes from '../../../styles/themes'
import lightTheme from '../../../styles/themes/light'

afterEach(cleanup)

describe('TextInput', () => {
  it('should render the Text Input without crashing', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <TextInput />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('input-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should render the optional icon component', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <TextInput inputIcon={<SvgCopy />} />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('icon-test')
    expect(el).toBeInTheDocument()
  })

  it('should render the optional units text', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <TextInput inputUnits='mb' />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('text-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should update input value on change', () => {
    const onChangeTextMock = jest.fn()
    const newInputText = 'test content'

    render(
      <ThemeProvider theme={themes.light}>
        <TextInput onChangeText={onChangeTextMock} />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('input-cmp')

    fireEvent.change(el, { target: { value: newInputText } })
    expect(onChangeTextMock).toHaveBeenCalled()
  })

  it('should render correct styling when input is disabled', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <TextInput type='disabled' />
      </ThemeProvider>,
    )

    const disabledStyle = lightTheme.placeholderText

    const el = screen.getByTestId('input-cmp')
    expect(el).toHaveStyle(`color: ${disabledStyle}`)
  })

  it('should hide the input text when hideText prop is set to true', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <TextInput hideText value='text for testing' />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('input-cmp')
    expect(el).toHaveStyle('color: transparent')
  })
})
