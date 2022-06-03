import { useMemo } from 'react'
import { useLiveQuery } from 'dexie-react-hooks'

import { useAppSelector } from '../../../../store/hooks'
import { selectNetwork } from '../../../../store/baseNode/selectors'
import getStatsRepository from '../../../../store/containers/statsRepository'
import { Container } from '../../../../store/containers/types'

const usePerformanceStats = () => {
  const configuredNetwork = useAppSelector(selectNetwork)
  const repository = useMemo(getStatsRepository, [])
  const stats = useLiveQuery(
    () => repository.getGroupedByContainer(configuredNetwork),
    [configuredNetwork, repository],
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
      {},
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
      memory,
      network,
    }),
    [cpu, memory, network],
  )
}

export default usePerformanceStats
