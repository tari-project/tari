import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../../../../styles/themes'
import t from '../../../../../locales'

import DateScheduler from './'

describe('DateScheduler', () => {
  it('should return selected days', () => {
    const onChange = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <DateScheduler onChange={onChange} />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByText(t.common.weekdayCapitals.monday))
    fireEvent.click(screen.getByText(t.common.weekdayCapitals.wednesday))

    expect(onChange).toHaveBeenCalledTimes(2)
    expect(onChange).toHaveBeenCalledWith({ days: [1], date: undefined })
    expect(onChange).toHaveBeenCalledWith({ days: [3], date: undefined })
  })

  it('should return selected days in order', () => {
    const onChange = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <DateScheduler onChange={onChange} days={[3]} />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByText(t.common.weekdayCapitals.monday))

    expect(onChange).toHaveBeenCalledTimes(1)
    expect(onChange).toHaveBeenCalledWith({ days: [1, 3], date: undefined })
  })

  it('should clear the date when user selects a day', () => {
    const onChange = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <DateScheduler onChange={onChange} date={new Date()} />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByText(t.common.weekdayCapitals.monday))

    expect(onChange).toHaveBeenCalledTimes(1)
    expect(onChange).toHaveBeenCalledWith({ days: [1], date: undefined })
  })
})
