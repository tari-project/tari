import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import Checkbox from './'

import themes from '../../styles/themes'

const TEST_LABEL = 'test label'

describe('Checkbox', () => {
  it('should render label', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <Checkbox checked={false} onChange={() => null}>
          {TEST_LABEL}
        </Checkbox>
      </ThemeProvider>,
    )

    expect(screen.getByText(TEST_LABEL)).toBeInTheDocument()
  })

  it('should render svg icon when checked', () => {
    const { container } = render(
      <ThemeProvider theme={themes.light}>
        <Checkbox checked={true} onChange={() => null}>
          {TEST_LABEL}
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
          {TEST_LABEL}
        </Checkbox>
      </ThemeProvider>,
    )

    const label = screen.getByText(TEST_LABEL)
    fireEvent.click(label)
    expect(onChange).toHaveBeenCalledWith(true)
  })

  it('should call onChange correctly when tick is clicked', () => {
    const onChange = jest.fn()
    const { container } = render(
      <ThemeProvider theme={themes.light}>
        <Checkbox checked={true} onChange={onChange}>
          {TEST_LABEL}
        </Checkbox>
      </ThemeProvider>,
    )

    const tickContainer = container.querySelector('svg')?.parentElement
    expect(tickContainer).toBeInTheDocument()
    fireEvent.click(tickContainer as unknown as Element)
    expect(onChange).toHaveBeenCalledWith(false)
  })
})
