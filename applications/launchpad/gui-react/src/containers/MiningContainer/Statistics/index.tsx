import { useState, useMemo } from 'react'
import { useTheme } from 'styled-components'

import Box from '../../../components/Box'
import CoinsList from '../../../components/CoinsList'
import ButtonSwitch from '../../../components/ButtonSwitch'
import Text from '../../../components/Text'
import BarChart from '../../../components/Charts/Bar'
import CloseIcon from '../../../styles/Icons/Close'
import ArrowDown from '../../../styles/Icons/ArrowBottom2'
import ArrowUp from '../../../styles/Icons/ArrowTop2'
import t from '../../../locales'

import { MiningStatisticsInterval } from './types'
import MiningIntervalPicker from './MiningIntervalPicker'

type AccountData = {
  balance: {
    value: number
    currency: string
  }
  delta: {
    percentage: number
    interval: MiningStatisticsInterval
  }
}[]

const Account = ({ data }: { data: AccountData }) => {
  const theme = useTheme()

  return (
    <div
      style={{
        display: 'flex',
        columnGap: theme.spacing(),
        marginBottom: theme.spacing(),
      }}
    >
      {data.map(({ balance, delta }) => {
        const deltaColor =
          delta.percentage <= 0 ? theme.error : theme.onTextLight

        return (
          <div key={balance.currency}>
            <CoinsList
              coins={[
                { amount: balance.value.toString(), unit: balance.currency },
              ]}
            />
            <div style={{ display: 'flex', alignItems: 'center' }}>
              {delta.percentage <= 0 && (
                <ArrowDown
                  width='24px'
                  height='24px'
                  color={deltaColor}
                  style={{ marginLeft: '-6px' }}
                />
              )}
              {delta.percentage > 0 && (
                <ArrowUp
                  width='24px'
                  height='24px'
                  color={deltaColor}
                  style={{ marginLeft: '-6px' }}
                />
              )}
              <Text as='span' type='smallMedium' color={deltaColor}>
                {delta.percentage}%
              </Text>
              <Text
                as='span'
                type='smallMedium'
                color={theme.secondary}
                style={{ display: 'inline-block', marginLeft: '4px' }}
              >
                {t.mining.statistics.deltas[delta.interval as string]}
              </Text>
            </div>
          </div>
        )
      })}
    </div>
  )
}

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
    <Box style={{ width: 866 }}>
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

const StatisticsContainer = ({ onClose }: { onClose: () => void }) => {
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
}: {
  open: boolean
  onClose: () => void
}) => {
  if (!open) {
    return null
  }

  return <StatisticsContainer onClose={onClose} />
}

export default StatisticsWrapper
