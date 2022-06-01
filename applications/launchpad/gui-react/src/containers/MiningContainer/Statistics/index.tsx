import { useState, useEffect } from 'react'

import { MiningStatisticsInterval, AccountData } from './types'
import Statistics from './Statistics'

const monthly = {
  getData: (d: Date) =>
    [...Array(new Date(d.getFullYear(), d.getMonth(), 0).getDate()).keys()].map(
      day => ({
        point: (day + 1).toString().padStart(2, '0'),
        xtr: (day + 1) * 2000 - 60 * (day + 1),
        xmr: (day + 1) * 200 - 10 * (day + 1),
      }),
    ),
  getAccountData: () =>
    [
      {
        balance: {
          value: 45500,
          currency: 'xtr',
        },
        delta: {
          percentage: 2.1,
          interval: 'monthly',
        },
      },
      {
        balance: {
          value: 430,
          currency: 'xmr',
        },
        delta: {
          percentage: -3.7,
          interval: 'monthly',
        },
      },
    ] as AccountData,
}

const yearly = {
  getData: () =>
    [...Array(12).keys()].map(month => ({
      point: (month + 1).toString().padStart(2, '0'),
      xtr: (month + 1) * 2000 - 60 * (month + 1),
      xmr: (month + 1) * 200 - 10 * (month + 1),
    })),
  getAccountData: () =>
    [
      {
        balance: {
          value: 6660000,
          currency: 'xtr',
        },
        delta: {
          percentage: -22.1,
          interval: 'yearly',
        },
      },
      {
        balance: {
          value: 72000,
          currency: 'xmr',
        },
        delta: {
          percentage: 0.7,
          interval: 'yearly',
        },
      },
    ] as AccountData,
}

const all = {
  getData: () =>
    [...Array(2).keys()].map(year => ({
      point: (year + 2021).toString(),
      xtr: (year + 1) * 2000 - 60 * (year + 1),
      xmr: (year + 1) * 200 - 10 * (year + 1),
    })),
  getAccountData: () =>
    [
      {
        balance: {
          value: 6660000,
          currency: 'xtr',
        },
        delta: {
          percentage: 0,
          interval: 'yearly',
        },
      },
      {
        balance: {
          value: 72000,
          currency: 'xmr',
        },
        delta: {
          percentage: 0,
          interval: 'yearly',
        },
      },
    ] as AccountData,
}

/**
 * @name StatisticsContainer
 * @description component responsible for getting statistics data from backend and passing them correctly to presentation component
 *
 * @prop {() => void} onClose - callback to be called when user wants to close statistics
 * @prop {() => void} [onReady] - callback to be called when presentation component is mounted and rendered for the first time
 */
const StatisticsContainer = ({
  onClose,
  onReady,
}: {
  onClose: () => void
  onReady?: () => void
}) => {
  const [interval, setInterval] = useState<MiningStatisticsInterval>('monthly')
  const [intervalToShow, setIntervalToShow] = useState(new Date())
  useEffect(() => {
    onReady && onReady()
  }, [])

  let data = monthly.getData(intervalToShow)
  let accountData: AccountData = monthly.getAccountData()

  if (interval === 'yearly') {
    data = yearly.getData()
    accountData = yearly.getAccountData()
  }

  if (interval === 'all') {
    data = all.getData()
    accountData = all.getAccountData()
  }

  return (
    <Statistics
      interval={interval}
      setInterval={setInterval}
      intervalToShow={intervalToShow}
      setIntervalToShow={setIntervalToShow}
      onClose={onClose}
      data={data}
      accountData={accountData}
    />
  )
}

const StatisticsWrapper = ({
  open,
  onClose,
  onReady,
}: {
  open: boolean
  onClose: () => void
  onReady?: () => void
}) => {
  if (!open) {
    return null
  }

  return <StatisticsContainer onClose={onClose} onReady={onReady} />
}

export default StatisticsWrapper
