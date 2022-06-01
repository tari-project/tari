import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'

import DatePicker from '.'

describe('Modal', () => {
  const someDateInMay2022 = new Date('2022-05-14T07:00:10.010Z')
  jest.useFakeTimers().setSystemTime(someDateInMay2022)

  it('should not render the component when open is false', () => {
    const { container } = render(
      <ThemeProvider theme={themes.light}>
        <DatePicker open={false} onChange={() => null} />
      </ThemeProvider>,
    )

    expect(container.childElementCount).toBe(0)
  })

  it('should render calendar component when open', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <DatePicker open={true} onChange={() => null} />
      </ThemeProvider>,
    )

    expect(screen.getByText('Sun')).toBeInTheDocument()
    expect(screen.getByText('Mon')).toBeInTheDocument()
    expect(screen.getByText('Tue')).toBeInTheDocument()
    expect(screen.getByText('Wed')).toBeInTheDocument()
    expect(screen.getByText('Thu')).toBeInTheDocument()
    expect(screen.getByText('Fri')).toBeInTheDocument()
    expect(screen.getByText('Sat')).toBeInTheDocument()
  })

  it('should render month of selected date', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <DatePicker
          open={true}
          onChange={() => null}
          value={new Date('2022-05-10')}
        />
      </ThemeProvider>,
    )

    expect(screen.getByText('May 2022')).toBeInTheDocument()
  })

  it('should call onChange on click', () => {
    const onChange = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <DatePicker
          open={true}
          onChange={onChange}
          value={new Date('2022-05-10')}
        />
      </ThemeProvider>,
    )

    const lastOfMay = screen.getByText('31')
    fireEvent.click(lastOfMay)

    expect(onChange).toHaveBeenCalledTimes(1)
    const selectedDate = onChange.mock.calls[0][0]

    expect(selectedDate.toISOString()).toBe('2022-05-31T00:00:00.000Z')
  })

  it('should not allow selecting dates in the past', () => {
    jest.useFakeTimers().setSystemTime(new Date('2022-05-08'))

    const onChange = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <DatePicker
          open={true}
          onChange={onChange}
          value={new Date('2022-05-10')}
        />
      </ThemeProvider>,
    )

    const inThePast = screen.getByText('7')
    const button = inThePast.closest('button')
    expect(button).toBeDisabled()

    fireEvent.click(inThePast)
    expect(onChange).not.toHaveBeenCalled()
  })
})
