import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'

import ButtonRadio from '.'

const defaultOptions = [
  { option: 'option1', label: 'Option 1' },
  { option: 'option2', label: 'Option 2' },
  { option: 'option3', label: 'Option 3' },
]

describe('ButtonRadio', () => {
  it('should render all the options', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <ButtonRadio
          value='option1'
          options={defaultOptions}
          onChange={() => null}
        />
      </ThemeProvider>,
    )

    defaultOptions.forEach(({ label }) => {
      expect(screen.queryByText(label)).toBeInTheDocument()
    })
  })

  it('should call onChange with correct value', () => {
    const onChange = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <ButtonRadio
          value='option1'
          options={defaultOptions}
          onChange={onChange}
        />
      </ThemeProvider>,
    )

    const option2Button = screen.getByText('Option 2')
    fireEvent.click(option2Button)

    expect(onChange).toHaveBeenCalledWith('option2')
  })

  it('should not render the component when option list is empty', () => {
    const { container } = render(
      <ThemeProvider theme={themes.light}>
        <ButtonRadio value='option1' options={[]} onChange={() => null} />
      </ThemeProvider>,
    )

    expect(container.childElementCount).toBe(0)
  })

  it('should not allow clicking disabled option', () => {
    const onChange = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <ButtonRadio
          value='option1'
          options={[
            { option: 'option1', label: 'Disabled option', disabled: true },
          ]}
          onChange={onChange}
        />
      </ThemeProvider>,
    )

    const option2Button = screen.getByText('Disabled option')
    fireEvent.click(option2Button)

    expect(onChange).not.toHaveBeenCalled()
  })
})
