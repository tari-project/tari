import { fireEvent, render, screen, act } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import Checkbox from './'

import themes from '../../styles/themes'

describe('Checkbox', () => {
  it('should render label', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Checkbox checked={false} onChange={() => null}>
          test label
        </Checkbox>
      </ThemeProvider>,
    )

    expect(screen.getByText('test label')).toBeInTheDocument()
  })

  it('should render svg icon when checked', () => {
    const { container } = render(
      <ThemeProvider theme={themes.light}>
        <Checkbox checked={true} onChange={() => null}>
          test
        </Checkbox>
      </ThemeProvider>,
    )

    const icon = container.querySelector('svg')
    expect(icon).toBeInTheDocument()
  })

  it('should call onChange correctly when label is clicked', () => {
    const onChange = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <Checkbox checked={false} onChange={onChange}>
          test
        </Checkbox>
      </ThemeProvider>,
    )

    const label = screen.getByText('test')
    fireEvent.click(label)
    expect(onChange).toHaveBeenCalledWith(true)
  })

  it('should call onChange correctly when tick is clicked', () => {
    const onChange = jest.fn()
    const { container } = render(
      <ThemeProvider theme={themes.light}>
        <Checkbox checked={true} onChange={onChange}>
          test
        </Checkbox>
      </ThemeProvider>,
    )

    const tickContainer = container.querySelector('svg')?.parentElement
    expect(tickContainer).toBeInTheDocument()
    fireEvent.click(tickContainer as unknown as Element)
    expect(onChange).toHaveBeenCalledWith(false)
  })
})
