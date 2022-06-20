import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../../../styles/themes'

import MiningIntervalPickerComponent from './MiningIntervalPickerComponent'

describe('MiningIntervalPickerComponent', () => {
  it('should render current month for monthly interval', () => {
    const expectedValue = 'Jun 2022'
    render(
      <ThemeProvider theme={themes.light}>
        <MiningIntervalPickerComponent
          value={new Date('2022-06-20')}
          interval='monthly'
          onChange={() => null}
          dataFrom={new Date('2022-05-01')}
          dataTo={new Date()}
        />
      </ThemeProvider>,
    )

    expect(screen.getByText(expectedValue)).toBeInTheDocument()
  })

  it('should render current year for yearly interval', () => {
    const expectedValue = '2022'
    render(
      <ThemeProvider theme={themes.light}>
        <MiningIntervalPickerComponent
          value={new Date('2022-06-20')}
          interval='yearly'
          onChange={() => null}
          dataFrom={new Date('2022-05-01')}
          dataTo={new Date()}
        />
      </ThemeProvider>,
    )

    expect(screen.getByText(expectedValue)).toBeInTheDocument()
  })

  it('should step by a year for yearly interval', () => {
    const onChange = jest.fn()

    render(
      <ThemeProvider theme={themes.light}>
        <MiningIntervalPickerComponent
          value={new Date('2022-06-20')}
          interval='yearly'
          onChange={onChange}
          dataFrom={new Date('2021-05-01')}
          dataTo={new Date('2023-06-21')}
        />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByTestId('iterator-btn-prev'))
    expect(onChange).toHaveBeenCalledTimes(1)
    expect(onChange.mock.calls[0][0].toISOString()).toBe(
      new Date('2021-06-20').toISOString(),
    )

    fireEvent.click(screen.getByTestId('iterator-btn-next'))
    expect(onChange).toHaveBeenCalledTimes(2)
    expect(onChange.mock.calls[1][0].toISOString()).toBe(
      new Date('2023-06-20').toISOString(),
    )
  })

  it('should not step if no previous or next year data is available for yearly interval', () => {
    const onChange = jest.fn()

    render(
      <ThemeProvider theme={themes.light}>
        <MiningIntervalPickerComponent
          value={new Date('2022-06-20')}
          interval='yearly'
          onChange={onChange}
          dataFrom={new Date('2022-05-01')}
          dataTo={new Date('2022-06-21')}
        />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByTestId('iterator-btn-prev'))
    fireEvent.click(screen.getByTestId('iterator-btn-next'))

    expect(onChange).toHaveBeenCalledTimes(0)
  })

  it('should step by a month for monthly interval', () => {
    const onChange = jest.fn()

    render(
      <ThemeProvider theme={themes.light}>
        <MiningIntervalPickerComponent
          value={new Date('2022-06-20')}
          interval='monthly'
          onChange={onChange}
          dataFrom={new Date('2021-05-01')}
          dataTo={new Date('2023-06-21')}
        />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByTestId('iterator-btn-prev'))
    expect(onChange).toHaveBeenCalledTimes(1)
    expect(onChange.mock.calls[0][0].toISOString()).toBe(
      new Date('2022-05-20').toISOString(),
    )

    fireEvent.click(screen.getByTestId('iterator-btn-next'))
    expect(onChange).toHaveBeenCalledTimes(2)
    expect(onChange.mock.calls[1][0].toISOString()).toBe(
      new Date('2022-07-20').toISOString(),
    )
  })

  it('should not step if no data for previous and next month', () => {
    const onChange = jest.fn()

    render(
      <ThemeProvider theme={themes.light}>
        <MiningIntervalPickerComponent
          value={new Date('2022-06-20')}
          interval='monthly'
          onChange={onChange}
          dataFrom={new Date('2022-06-01')}
          dataTo={new Date('2022-06-20')}
        />
      </ThemeProvider>,
    )

    fireEvent.click(screen.getByTestId('iterator-btn-prev'))
    fireEvent.click(screen.getByTestId('iterator-btn-next'))
    expect(onChange).toHaveBeenCalledTimes(0)
  })
})
