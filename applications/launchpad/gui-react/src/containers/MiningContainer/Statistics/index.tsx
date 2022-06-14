import { useMemo, useState, useEffect } from 'react'

import getTransactionsRepository, {
  DataResolution,
  MinedTariEntry,
} from '../../../persistence/transactionsRepository'
import * as DateUtils from '../../../utils/Date'

import { MiningStatisticsInterval, AccountData } from './types'
import Statistics from './Statistics'

const transactionsRepository = getTransactionsRepository()

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
  const [interval, setInterval] = useState<MiningStatisticsInterval>('monthly')
  const [intervalToShow, setIntervalToShow] = useState(new Date())
  useEffect(() => {
    onReady && onReady()
  }, [])

  const [disableAllFilter, setDisableAllFilter] = useState(false)
  useEffect(() => {
    const doTheThing = async () => {
      const hasDataBeforeCurrentYear =
        await transactionsRepository.hasDataBefore(
          DateUtils.startOfYear(new Date()),
        )
      setDisableAllFilter(!hasDataBeforeCurrentYear)
    }
    doTheThing()
  }, [])

  const from = useMemo(
    () => getFrom(interval, intervalToShow),
    [interval, intervalToShow],
  )
  const to = useMemo(
    () => getTo(interval, intervalToShow),
    [interval, intervalToShow],
  )

  const [data, setData] = useState<MinedTariEntry[]>([])
  useEffect(() => {
    const resolution = {
      monthly: DataResolution.Daily,
      yearly: DataResolution.Monthly,
      all: DataResolution.Yearly,
    }[interval]

    const getData = async () => {
      const results = await transactionsRepository.getMinedXtr(
        from,
        to,
        resolution,
      )

      if (interval === 'monthly') {
        const year = intervalToShow.getFullYear()
        const month = intervalToShow.getMonth() + 1
        setData(
          [...Array(new Date(year, month, 0).getDate()).keys()]
            .map(day => {
              const when = `${year}-${month.toString().padStart(2, '0')}-${(
                day + 1
              )
                .toString()
                .padStart(2, '0')}`

              return (
                results[when] || {
                  when,
                  xtr: 0,
                }
              )
            })
            .map(({ when, xtr }) => ({
              xtr,
              when: when.substring(8),
            })),
        )
      }

      if (interval === 'yearly') {
        const year = intervalToShow.getFullYear()
        setData(
          [...Array(12).keys()]
            .map(month => {
              const when = `${year}-${(month + 1).toString().padStart(2, '0')}`

              return (
                results[when] || {
                  when,
                  xtr: 0,
                }
              )
            })
            .map(({ when, xtr }) => ({
              xtr,
              when: when.substring(5),
            })),
        )
      }

      if (interval === 'all') {
        setData(Object.values(results))
      }
    }
    getData()
  }, [from, to, interval])
  const [accountData, setAccountData] = useState<AccountData>([])
  useEffect(() => {
    const getAccountData = async () => {
      if (interval === 'monthly') {
        const currentMonthStart = DateUtils.startOfMonth(intervalToShow)
        const currentMonthPromise = transactionsRepository.getMinedXtr(
          currentMonthStart,
          DateUtils.endOfMonth(intervalToShow),
          DataResolution.Monthly,
        )
        const previousMonthStart = new Date(
          `${currentMonthStart.getFullYear()}-${currentMonthStart.getMonth()}`,
        )
        const previousMonthPromise = transactionsRepository.getMinedXtr(
          previousMonthStart,
          DateUtils.endOfMonth(previousMonthStart),
          DataResolution.Monthly,
        )

        const [currentMonth, previousMonth] = await Promise.all([
          currentMonthPromise,
          previousMonthPromise,
        ])

        const monthlyAccountData: AccountData = [
          {
            balance: {
              value: currentMonth[0].xtr,
              currency: 'xtr',
            },
            delta: {
              percentage:
                ((previousMonth[0].xtr - currentMonth[0].xtr) /
                  previousMonth[0].xtr) *
                100,
              interval,
            },
          },
        ]
        setAccountData(monthlyAccountData)
      }

      if (interval === 'yearly') {
        const currentYearStart = DateUtils.startOfYear(intervalToShow)
        const currentYearPromise = transactionsRepository.getMinedXtr(
          currentYearStart,
          DateUtils.endOfYear(intervalToShow),
          DataResolution.Yearly,
        )
        const previousYearStart = new Date(
          `${currentYearStart.getFullYear() - 1}`,
        )
        const previousYearPromise = transactionsRepository.getMinedXtr(
          previousYearStart,
          DateUtils.endOfMonth(previousYearStart),
          DataResolution.Yearly,
        )

        const [currentYear, previousYear] = await Promise.all([
          currentYearPromise,
          previousYearPromise,
        ])

        const yearlyAccountData: AccountData = [
          {
            balance: {
              value: currentYear[0].xtr,
              currency: 'xtr',
            },
            delta: {
              percentage:
                ((previousYear[0].xtr - currentYear[0].xtr) /
                  previousYear[0].xtr) *
                100,
              interval,
            },
          },
        ]
        setAccountData(yearlyAccountData)
      }
    }
    getAccountData()
  }, [from, to, interval])

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
      dataFrom={from}
      dataTo={to}
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
