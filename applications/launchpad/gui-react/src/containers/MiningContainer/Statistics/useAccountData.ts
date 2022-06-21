import { useState, useEffect } from 'react'

import useTransactionsRepository, {
  DataResolution,
} from '../../../persistence/transactionsRepository'
import * as DateUtils from '../../../utils/Date'

import { MiningStatisticsInterval, AccountData } from './types'

const useAccountData = ({
  interval,
  intervalToShow,
  from,
  to,
}: {
  interval: MiningStatisticsInterval
  intervalToShow: Date
  from: Date
  to: Date
}) => {
  const transactionsRepository = useTransactionsRepository()
  const [accountData, setAccountData] = useState<AccountData>([])
  useEffect(() => {
    const getAccountData = async () => {
      if (interval === 'monthly') {
        const currentMonthStart = DateUtils.startOfMonth(intervalToShow)
        const currentMonthPromise = transactionsRepository
          .getMinedXtr(
            currentMonthStart,
            DateUtils.endOfMonth(intervalToShow),
            DataResolution.Monthly,
          )
          .then(Object.values)
        const previousMonthStart = new Date(
          `${currentMonthStart.getFullYear()}-${currentMonthStart
            .getMonth()
            .toString()
            .padStart(2, '0')}`,
        )
        const previousMonthPromise = transactionsRepository
          .getMinedXtr(
            previousMonthStart,
            DateUtils.endOfMonth(previousMonthStart),
            DataResolution.Monthly,
          )
          .then(Object.values)

        const [currentMonth, previousMonth] = await Promise.all([
          currentMonthPromise,
          previousMonthPromise,
        ])

        const currentMonthBalance = currentMonth[0]?.xtr || 0
        const previousMonthBalance = previousMonth[0]?.xtr || 0
        const monthlyAccountData: AccountData = [
          {
            balance: {
              value: currentMonthBalance,
              currency: 'xtr',
            },
            delta: {
              percentage: Boolean(previousMonthBalance),
              value: previousMonthBalance
                ? ((currentMonthBalance - previousMonthBalance) /
                    previousMonthBalance) *
                  100
                : currentMonthBalance,
              interval,
            },
          },
        ]
        setAccountData(monthlyAccountData)
      }

      if (interval === 'yearly') {
        const currentYearStart = DateUtils.startOfYear(intervalToShow)
        const currentYearPromise = transactionsRepository
          .getMinedXtr(
            currentYearStart,
            DateUtils.endOfYear(intervalToShow),
            DataResolution.Yearly,
          )
          .then(Object.values)
        const previousYearStart = new Date(
          `${currentYearStart.getFullYear() - 1}`,
        )
        const previousYearPromise = transactionsRepository
          .getMinedXtr(
            previousYearStart,
            DateUtils.endOfYear(previousYearStart),
            DataResolution.Yearly,
          )
          .then(Object.values)

        const [currentYear, previousYear] = await Promise.all([
          currentYearPromise,
          previousYearPromise,
        ])

        const currentYearBalance = currentYear[0]?.xtr || 0
        const previousYearBalance = previousYear[0]?.xtr || 0
        const yearlyAccountData: AccountData = [
          {
            balance: {
              value: currentYear[0].xtr,
              currency: 'xtr',
            },
            delta: {
              percentage: Boolean(previousYearBalance),
              value: previousYearBalance
                ? ((currentYearBalance - previousYearBalance) /
                    previousYearBalance) *
                  100
                : previousYearBalance,
              interval,
            },
          },
        ]
        setAccountData(yearlyAccountData)
      }

      if (interval === 'all') {
        const currentBalance =
          await transactionsRepository.getLifelongMinedBalance()

        const yearlyAccountData: AccountData = [
          {
            balance: {
              value: currentBalance,
              currency: 'xtr',
            },
            delta: {
              percentage: false,
              value: 0,
              interval,
            },
          },
        ]
        setAccountData(yearlyAccountData)
      }
    }
    getAccountData()
  }, [from, to, interval, intervalToShow])

  return accountData
}

export default useAccountData
