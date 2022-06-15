import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../../../styles/themes'
import t from '../../../../locales'
import { AccountData } from '../types'

import Account from '.'

describe('Account', () => {
  it('should render all the balances', () => {
    const accountData = [
      {
        balance: { value: 100, currency: 'xtr' },
        delta: { percentage: true, value: 1, interval: 'monthly' },
      },
      {
        balance: { value: 200, currency: 'xmr' },
        delta: { percentage: true, value: 2, interval: 'monthly' },
      },
    ] as AccountData
    render(
      <ThemeProvider theme={themes.light}>
        <Account data={accountData} />
      </ThemeProvider>,
    )

    expect(screen.getByText('100')).toBeInTheDocument()
    expect(screen.getByText('xtr')).toBeInTheDocument()
    expect(screen.getByText('200')).toBeInTheDocument()
    expect(screen.getByText('xmr')).toBeInTheDocument()
    expect(screen.getAllByText(t.mining.statistics.deltas.monthly).length).toBe(
      2,
    )
  })

  it('should not render deltas if percentage is 0', () => {
    const accountData = [
      {
        balance: { value: 100, currency: 'xtr' },
        delta: { percentage: true, value: 0, interval: 'monthly' },
      },
      {
        balance: { value: 200, currency: 'xmr' },
        delta: { percentage: true, value: 0, interval: 'monthly' },
      },
    ] as AccountData
    render(
      <ThemeProvider theme={themes.light}>
        <Account data={accountData} />
      </ThemeProvider>,
    )
    expect(
      screen.queryByText(t.mining.statistics.deltas.monthly),
    ).not.toBeInTheDocument()
  })

  it('should render non percentage delta', () => {
    const accountData = [
      {
        balance: { value: 100, currency: 'xtr' },
        delta: { percentage: false, value: 236, interval: 'monthly' },
      },
    ] as AccountData
    render(
      <ThemeProvider theme={themes.light}>
        <Account data={accountData} />
      </ThemeProvider>,
    )

    expect(screen.getByText('100')).toBeInTheDocument()
    expect(screen.getByText('236')).toBeInTheDocument()
    expect(screen.getAllByText('xtr').length).toBe(2)
    expect(
      screen.getByText(t.mining.statistics.deltas.monthly),
    ).toBeInTheDocument()
  })
})
