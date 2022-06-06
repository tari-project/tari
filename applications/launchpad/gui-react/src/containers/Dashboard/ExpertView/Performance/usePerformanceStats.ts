import { useEffect, useMemo, useState } from 'react'

import { useAppSelector } from '../../../../store/hooks'
import { selectNetwork } from '../../../../store/baseNode/selectors'
import getStatsRepository, {
  StatsEntry,
} from '../../../../store/containers/statsRepository'
import { Container } from '../../../../store/containers/types'
import { Dictionary } from '../../../../types/general'

const usePerformanceStats = ({
  enabled,
  from,
  to,
  extractor,
}: {
  enabled: boolean
  from: Date
  to: Date
  extractor: (entry: StatsEntry) => { timestamp: string; value: number }
}): Record<Container, { timestamp: string; value: number }[]> => {
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
