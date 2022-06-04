import { useMemo } from 'react'
import { useLiveQuery } from 'dexie-react-hooks'

import { useAppSelector } from '../../../../store/hooks'
import { selectNetwork } from '../../../../store/baseNode/selectors'
import getStatsRepository from '../../../../store/containers/statsRepository'
import { Container } from '../../../../store/containers/types'

const usePerformanceStats = ({
  from,
  to,
}: {
  from: Date
  to: Date
}): {
  cpu: Record<Container, { timestamp: string; cpu: number }[]>
} => {
  const configuredNetwork = useAppSelector(selectNetwork)
  const repository = useMemo(getStatsRepository, [])
  const stats = useLiveQuery(
    () => repository.getGroupedByContainer(configuredNetwork, from, to),
    [configuredNetwork, repository, from, to],
  )

  const cpu = useMemo(() => {
    return Object.values(Container).reduce(
      (accu, current) => ({
        ...accu,
        [current]: ((stats && stats[current]) || []).map(
          ({ cpu, timestamp }) => ({
            cpu,
            timestamp,
          }),
        ),
      }),
      {} as Record<Container, { timestamp: string; cpu: number }[]>,
    )
  }, [stats])

  const memory = useMemo(() => {
    return Object.values(Container).reduce(
      (accu, current) => ({
        ...accu,
        [current]: ((stats && stats[current]) || []).map(
          ({ memory, timestamp }) => ({
            memory,
            timestamp,
          }),
        ),
      }),
      {},
    )
  }, [stats])

  const network = useMemo(() => {
    return Object.values(Container).reduce(
      (accu, current) => ({
        ...accu,
        [current]: ((stats && stats[current]) || []).map(
          ({ upload, download, timestamp }) => ({
            upload,
            download,
            timestamp,
          }),
        ),
      }),
      {},
    )
  }, [stats])

  return useMemo(
    () => ({
      cpu,
    }),
    [cpu],
  )
}

export default usePerformanceStats
