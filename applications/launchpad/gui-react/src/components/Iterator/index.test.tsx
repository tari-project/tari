import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'

import Iterator from '.'

describe('Iterator', () => {
  it('should render current value', () => {
    const currentValue = 'current value'
    render(
      <ThemeProvider theme={themes.light}>
        <Iterator
          value={currentValue}
          next={() => null}
          previous={() => null}
        />
      </ThemeProvider>,
    )

    expect(screen.getByText(currentValue)).toBeInTheDocument()
  })

  it('should call callbacks when next/prev buttons clicked', () => {
    const next = jest.fn()
    const previous = jest.fn()

    render(
      <ThemeProvider theme={themes.light}>
        <Iterator value='value' next={next} previous={previous} />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByTestId('iterator-btn-prev'))
    fireEvent.click(screen.getByTestId('iterator-btn-next'))

    expect(next).toHaveBeenCalledTimes(1)
    expect(previous).toHaveBeenCalledTimes(1)
  })

  it('should not render next/prev buttons if no more values available', () => {
    const next = jest.fn()
    const previous = jest.fn()

    render(
      <ThemeProvider theme={themes.light}>
        <Iterator
          value='value'
          next={next}
          previous={previous}
          hasNext={false}
          hasPrevious={false}
        />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByTestId('iterator-btn-prev'))
    fireEvent.click(screen.getByTestId('iterator-btn-next'))

    expect(next).toHaveBeenCalledTimes(0)
    expect(previous).toHaveBeenCalledTimes(0)
  })
})
