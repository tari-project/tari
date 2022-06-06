import { useEffect, useMemo, useState } from 'react'

import { useAppSelector } from '../../../../store/hooks'
import { selectNetwork } from '../../../../store/baseNode/selectors'
import getStatsRepository, {
  StatsEntry,
} from '../../../../store/containers/statsRepository'
import { Container } from '../../../../store/containers/types'
import { Dictionary } from '../../../../types/general'

const usePerformanceStats = ({
  refreshRate,
  from,
  to,
}: {
  refreshRate: number
  from: Date
  to: Date
}): {
  cpu: Record<Container, { timestamp: string; cpu: number }[]>
} => {
  const configuredNetwork = useAppSelector(selectNetwork)
  const repository = useMemo(getStatsRepository, [])
  const [stats, setStats] = useState<Dictionary<StatsEntry[]>>()

  useEffect(() => {
    const thing = async () => {
      const stats = await repository.getGroupedByContainer(
        configuredNetwork,
        from,
        to,
      )

      setStats(stats)
    }

    thing()
  }, [refreshRate, from, to])

  const cpu = useMemo(() => {
    const r = Object.values(Container).reduce(
      (accu, current) => ({
        ...accu,
        [current]: ((stats && stats[current]) || []).map(
          ({ cpu, timestamp }: StatsEntry) => ({
            cpu,
            timestamp,
          }),
        ),
      }),
      {} as Record<Container, { timestamp: string; cpu: number }[]>,
    )

    return r
  }, [stats])

  return useMemo(
    () => ({
      cpu,
    }),
    [cpu],
  )
}

export default usePerformanceStats
