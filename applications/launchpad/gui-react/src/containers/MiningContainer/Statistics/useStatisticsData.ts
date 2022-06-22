import { useState, useEffect } from 'react'

import useTransactionsRepository, {
  DataResolution,
  MinedTariEntry,
} from '../../../persistence/transactionsRepository'

import { MiningStatisticsInterval } from './types'

const useStatisticsData = ({
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
  }, [from, to, interval, intervalToShow])

  return data
}

export default useStatisticsData
