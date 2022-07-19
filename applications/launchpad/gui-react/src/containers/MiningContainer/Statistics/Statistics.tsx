import { useTheme } from 'styled-components'

import Box from '../../../components/Box'
import ButtonRadio from '../../../components/ButtonRadio'
import Text from '../../../components/Text'
import BarChart from '../../../components/Charts/Bar'
import CloseIcon from '../../../styles/Icons/Close'
import t from '../../../locales'
import { MinedTariEntry } from '../../../persistence/transactionsRepository'

import { MiningStatisticsInterval, AccountData } from './types'
import MiningIntervalPicker from './MiningIntervalPicker'
import Account from './Account'

const intervalOptions = (disableAllFilter?: boolean) => [
  { option: 'monthly', label: t.mining.statistics.intervals.monthly },
  { option: 'yearly', label: t.mining.statistics.intervals.yearly },
  {
    option: 'all',
    label: t.mining.statistics.intervals.all,
    disabled: disableAllFilter,
  },
]

/**
 * @name Statistics
 * @description Presentation component for showing mining statistics data
 *
 * @prop {MiningStatisticsInterval} interval - what time period statistics relate to
 * @prop {(i: MiningStatisticsInterval) => void} setInterval - setter of statistics time period
 * @prop {Date} intervalToShow - representation of time period (month / year) to allow user to navigate between different periods
 * @prop {(d: Date) => void} setIntervalToShow - setter for intervalToShow
 * @prop {() => void} onClose - callback when user closes statistics
 * @prop {MinedTariEntry[]} data - period data
 * @prop {AccountData} accountData - data regarding coin balances and percentage changes period to period
 * @prop {boolean} [disableAllFilter] - whether 'all' filter should be disabled - happens when there is only data for one year
 */
const Statistics = ({
  interval,
  setInterval,
  intervalToShow,
  setIntervalToShow,
  onClose,
  data,
  accountData,
  disableAllFilter,
}: {
  interval: MiningStatisticsInterval
  setInterval: (i: MiningStatisticsInterval) => void
  intervalToShow: Date
  setIntervalToShow: (d: Date) => void
  onClose: () => void
  data: MinedTariEntry[]
  accountData: AccountData
  disableAllFilter?: boolean
}) => {
  const theme = useTheme()

  return (
    <Box
      style={{
        width: 866,
        maxWidth: '100%',
        background: theme.nodeBackground,
        border: `1px solid ${theme.selectBorderColor}`,
      }}
    >
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          marginBottom: theme.spacing(),
        }}
      >
        <Text type='defaultHeavy'>{t.mining.statistics.title}</Text>
        <div onClick={onClose} style={{ cursor: 'pointer' }}>
          <CloseIcon height='24px' width='24px' color={theme.helpTipText} />
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
        <ButtonRadio
          value={interval}
          onChange={intervalString =>
            setInterval(intervalString as MiningStatisticsInterval)
          }
          options={intervalOptions(disableAllFilter)}
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
        data={data as unknown as Record<string, string | number>[]}
        indexBy={'when'}
        keys={['xtr']}
        style={{ width: '100%', height: 250 }}
      />
    </Box>
  )
}

export default Statistics
