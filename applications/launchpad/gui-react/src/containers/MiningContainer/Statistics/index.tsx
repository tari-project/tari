import { useMemo, useState, useEffect } from 'react'

import * as DateUtils from '../../../utils/Date'
import useTransactionsRepository from '../../../persistence/transactionsRepository'

import { MiningStatisticsInterval } from './types'
import Statistics from './Statistics'
import useStatisticsData from './useStatisticsData'
import useAccountData from './useAccountData'

const getFrom = (
  interval: MiningStatisticsInterval,
  dateInInterval: Date,
): Date => {
  switch (interval) {
    case 'monthly':
      return DateUtils.startOfMonth(dateInInterval)
    case 'yearly':
      return DateUtils.startOfYear(dateInInterval)
    case 'all':
      return new Date('1970')
  }
}

const getTo = (
  interval: MiningStatisticsInterval,
  dateInInterval: Date,
): Date => {
  switch (interval) {
    case 'monthly':
      return DateUtils.endOfMonth(dateInInterval)
    case 'yearly':
      return DateUtils.endOfYear(dateInInterval)
    case 'all':
      return new Date()
  }
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
  const transactionsRepository = useTransactionsRepository()
  const [interval, setInterval] = useState<MiningStatisticsInterval>('monthly')
  const [intervalToShow, setIntervalToShow] = useState(new Date())
  useEffect(() => {
    onReady && onReady()
  }, [])

  const [disableAllFilter, setDisableAllFilter] = useState(false)
  useEffect(() => {
    const calculateAllFilterDisabled = async () => {
      const hasDataBeforeCurrentYear =
        await transactionsRepository.hasDataBefore(
          DateUtils.startOfYear(new Date()),
        )
      setDisableAllFilter(!hasDataBeforeCurrentYear)
    }
    calculateAllFilterDisabled()
  }, [])

  const from = useMemo(
    () => getFrom(interval, intervalToShow),
    [interval, intervalToShow],
  )
  const to = useMemo(
    () => getTo(interval, intervalToShow),
    [interval, intervalToShow],
  )

  const data = useStatisticsData({ from, to, interval, intervalToShow })
  const accountData = useAccountData({ from, to, interval, intervalToShow })

  return (
    <Statistics
      disableAllFilter={disableAllFilter}
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
