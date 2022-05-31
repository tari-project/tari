import { useMemo } from 'react'

import Box from '../../../components/Box'
import BarChart from '../../../components/Charts/Bar'

const Statistics = ({
  onClose,
  data,
}: {
  onClose: () => void
  data: Record<string, string | number>[]
}) => {
  return (
    <Box style={{ width: 866 }}>
      <p>
        chart? <button onClick={onClose}>close</button>
      </p>
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

  return <Statistics onClose={onClose} data={data} />
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
