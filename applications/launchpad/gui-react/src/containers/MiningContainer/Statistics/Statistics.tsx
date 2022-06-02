import { useTheme } from 'styled-components'

import Box from '../../../components/Box'
import ButtonRadio from '../../../components/ButtonRadio'
import Text from '../../../components/Text'
import BarChart from '../../../components/Charts/Bar'
import CloseIcon from '../../../styles/Icons/Close'
import t from '../../../locales'

import { MiningStatisticsInterval, AccountData } from './types'
import MiningIntervalPicker from './MiningIntervalPicker'
import Account from './Account'

const intervalOptions = (disableAllFilter?: boolean) => [
  {
    option: 'all',
    label: t.mining.statistics.intervals.all,
    disabled: disableAllFilter,
  },
  { option: 'monthly', label: t.mining.statistics.intervals.monthly },
  { option: 'yearly', label: t.mining.statistics.intervals.yearly },
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
 * @prop {Record<string, string | number>[]} data - period data
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
  dataFrom,
  dataTo,
}: {
  interval: MiningStatisticsInterval
  setInterval: (i: MiningStatisticsInterval) => void
  intervalToShow: Date
  setIntervalToShow: (d: Date) => void
  onClose: () => void
  data: Record<string, string | number>[]
  accountData: AccountData
  disableAllFilter?: boolean
  dataFrom: Date
  dataTo: Date
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
        <Text type='defaultHeavy'>{t.mining.statistics.title}</Text>
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
          dataFrom={dataFrom}
          dataTo={dataTo}
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
