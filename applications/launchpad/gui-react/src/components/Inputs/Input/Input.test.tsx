import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import Input from '.'
import SvgCopy from '../../../styles/Icons/Copy'

import themes from '../../../styles/themes'
import lightTheme from '../../../styles/themes/light'

afterEach(cleanup)

describe('Input', () => {
  it('should render the Text Input without crashing', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Input />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('input-cmp')
    expect(el).toBeInTheDocument()
  })

  it('should render the optional icon component', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Input inputIcon={<SvgCopy />} />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('icon-test')
    expect(el).toBeInTheDocument()
  })

  it('should render the optional units text', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Input inputUnits='mb' />
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
        <Input onChange={onChangeTextMock} />
      </ThemeProvider>,
    )

    const el = screen.getByTestId('input-cmp')

    fireEvent.change(el, { target: { value: newInputText } })
    expect(onChangeTextMock).toHaveBeenCalledWith(newInputText)
  })

  it('should render correct styling when input is disabled', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Input disabled />
      </ThemeProvider>,
    )

    const disabledStyle = lightTheme.placeholderText

    const el = screen.getByTestId('input-cmp')
    expect(el).toHaveStyle(`color: ${disabledStyle}`)
  })

  it('should not call icon click handler when disabled', () => {
    const onIconClick = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <Input disabled onIconClick={onIconClick} inputIcon={<SvgCopy />} />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByTestId('icon-test'))

    expect(onIconClick).not.toHaveBeenCalled()
  })

  it('should render error when given', () => {
    const errorText = 'This field is invalid - test text'
    render(
      <ThemeProvider theme={themes.light}>
        <Input error={errorText} />
      </ThemeProvider>,
    )

    const el = screen.getByText(errorText)
    expect(el).toBeInTheDocument()
  })

  it('should render label when given', () => {
    const labelText = 'This is text label'
    render(
      <ThemeProvider theme={themes.light}>
        <Input label={labelText} />
      </ThemeProvider>,
    )

    const el = screen.getByText(labelText)
    expect(el).toBeInTheDocument()
  })
})
