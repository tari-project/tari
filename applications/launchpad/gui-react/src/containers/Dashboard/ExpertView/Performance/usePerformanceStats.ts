import { useMemo, useRef, useEffect, useState } from 'react'

import { useAppSelector } from '../../../../store/hooks'
import { selectNetwork } from '../../../../store/baseNode/selectors'
import getStatsRepository from '../../../../store/containers/statsRepository'
import { Container, StatsDbEntry } from '../../../../store/containers/types'

// TODO implement against actual db https://github.com/Altalogy/tari/issues/46
// THIS is a temporary implementation just to show the flow
// when statsRepository is using Dexie.js, we will be able to use to liveQuery
// https://dexie.org/docs/dexie-react-hooks/useLiveQuery()
const usePerformanceStats = () => {
  const configuredNetwork = useAppSelector(selectNetwork)
  const repository = useMemo(getStatsRepository, [])
  const [allStats, setStats] = useState<Record<Container, StatsDbEntry[]>>()
  const interval = useRef<ReturnType<typeof setInterval> | undefined>()

  useEffect(() => {
    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    interval.current = setInterval(async () => {
      const stats = await repository.getGroupedByContainer(configuredNetwork)

      setStats(stats)
    }, 1000)

    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    return () => clearInterval(interval.current!)
  }, [repository, configuredNetwork])

  const cpu = useMemo(() => {
    return Object.values(Container).reduce(
      (accu, current) => ({
        ...accu,
        [current]: ((allStats && allStats[current]) || []).map(
          ({ cpu, timestamp }) => ({
            cpu,
            timestamp,
          }),
        ),
      }),
      {},
    )
  }, [allStats])

  const memory = useMemo(() => {
    return Object.values(Container).reduce(
      (accu, current) => ({
        ...accu,
        [current]: ((allStats && allStats[current]) || []).map(
          ({ memory, timestamp }) => ({
            memory,
            timestamp,
          }),
        ),
      }),
      {},
    )
  }, [allStats])

  const network = useMemo(() => {
    return Object.values(Container).reduce(
      (accu, current) => ({
        ...accu,
        [current]: ((allStats && allStats[current]) || []).map(
          ({ network: { upload, download }, timestamp }) => ({
            upload,
            download,
            timestamp,
          }),
        ),
      }),
      {},
    )
  }, [allStats])

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
