import { useEffect, useMemo, useState } from 'react'

import { useAppSelector } from '../../../../store/hooks'
import { selectNetwork } from '../../../../store/baseNode/selectors'
import getStatsRepository, {
  StatsEntry,
} from '../../../../persistence/statsRepository'
import { Container } from '../../../../store/containers/types'
import { Dictionary } from '../../../../types/general'

import { UsePerformanceStatsType } from './types'

/**
 * @name usePerformanceStats
 * @description hook hiding call to performance stats repositories and memoizing specific data used by extractor function
 *
 * @prop {boolean} enabled - if this is false, new data will not be queried, even if from/to change
 * @prop {Date} from - start of the time window to query, change of this prop refetches the data
 * @prop {Date} to - end of the time window to query, change of this prop refetches the data
 * @prop {StatsExtractorFunction} extractor - function to get specific values as timeseries
 */
const usePerformanceStats: UsePerformanceStatsType = ({
  enabled,
  from,
  to,
  extractor,
}) => {
  const configuredNetwork = useAppSelector(selectNetwork)
  const repository = useMemo(getStatsRepository, [])
  const [stats, setStats] = useState<Dictionary<StatsEntry[]>>()

  useEffect(() => {
    if (!enabled) {
      return
    }

    const thing = async () => {
      const stats = await repository.getGroupedByContainer(
        configuredNetwork,
        from,
        to,
      )

      setStats(stats)
    }

    thing()
  }, [enabled, from, to])

  const extracted = useMemo(() => {
    const r = Object.values(Container)
      .filter(container => Boolean(stats && stats[container]))
      .reduce(
        (accu, current) => ({
          ...accu,
          [current]: ((stats && stats[current]) || []).map(extractor),
        }),
        {} as Record<Container, { timestamp: string; value: number }[]>,
      )

    return r
  }, [stats])

  return extracted
}

export default usePerformanceStats
