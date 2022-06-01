import { useState, useMemo, useEffect } from 'react'

import { AccountData } from './types'
import Statistics from './Statistics'

const StatisticsContainer = ({
  onClose,
  onReady,
}: {
  onClose: () => void
  onReady?: () => void
}) => {
  const data = useMemo(
    () =>
      [...Array(31).keys()].map(day => ({
        day: (day + 1).toString().padStart(2, '0'),
        xtr: (day + 1) * 2000 - 60 * (day + 1),
        xmr: (day + 1) * 200 - 10 * (day + 1),
      })),
    [],
  )
  const [interval, setInterval] = useState('monthly')
  const [intervalToShow, setIntervalToShow] = useState(new Date())
  useEffect(() => {
    onReady && onReady()
  }, [])

  const accountData = [
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
  ] as AccountData

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
