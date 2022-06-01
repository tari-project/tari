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
      <div>
        <Account data={accountData} />
      </div>
      <BarChart
        data={data}
        indexBy={'point'}
        keys={['xtr', 'xmr']}
        style={{ width: '100%', height: 250 }}
      />
    </Box>
  )
}

export default Statistics
