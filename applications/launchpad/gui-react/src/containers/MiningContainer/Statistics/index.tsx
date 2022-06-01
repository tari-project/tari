import { useState, useMemo, useEffect } from 'react'
import { useTheme } from 'styled-components'

import Box from '../../../components/Box'
import ButtonSwitch from '../../../components/ButtonSwitch'
import Text from '../../../components/Text'
import BarChart from '../../../components/Charts/Bar'
import CloseIcon from '../../../styles/Icons/Close'
import t from '../../../locales'

import { MiningStatisticsInterval, AccountData } from './types'
import MiningIntervalPicker from './MiningIntervalPicker'
import Account from './Account'

const intervalOptions = [
  { option: 'all', label: t.mining.statistics.intervals.all },
  { option: 'monthly', label: t.mining.statistics.intervals.monthly },
  { option: 'yearly', label: t.mining.statistics.intervals.yearly },
]
const Statistics = ({
  interval,
  setInterval,
  intervalToShow,
  setIntervalToShow,
  onClose,
  data,
  accountData,
}: {
  interval: string
  setInterval: (i: string) => void
  intervalToShow: Date
  setIntervalToShow: (d: Date) => void
  onClose: () => void
  data: Record<string, string | number>[]
  accountData: AccountData
}) => {
  const theme = useTheme()

  return (
    <Box style={{ width: 866, maxWidth: '100%' }}>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          marginBottom: theme.spacing(),
        }}
      >
        <Text type='defaultHeavy'>Mined coins</Text>
        <div onClick={onClose} style={{ cursor: 'pointer' }}>
          <CloseIcon height='24px' width='24px' color={theme.secondary} />
        </div>
      </div>
      <div
        style={{
          display: 'flex',
          flexWrap: 'wrap',
          rowGap: theme.spacing(),
          justifyContent: 'space-between',
          marginBottom: theme.spacing(),
        }}
      >
        <ButtonSwitch
          value={interval}
          onChange={setInterval}
          options={intervalOptions}
        />
        <MiningIntervalPicker
          value={intervalToShow}
          interval={interval as MiningStatisticsInterval}
          onChange={setIntervalToShow}
        />
      </div>
      {interval !== 'all' && (
        <div>
          <Account data={accountData} />
        </div>
      )}
      <BarChart
        data={data}
        indexBy={'day'}
        keys={['xtr', 'xmr']}
        style={{ width: '100%', height: 250 }}
      />
    </Box>
  )
}

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
