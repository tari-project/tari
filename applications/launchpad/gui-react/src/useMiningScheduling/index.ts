import { useEffect, useRef, useCallback, useState } from 'react'

import { startOfMinute } from '../utils/Date'
import { MiningNodeType, Schedule } from '../types/general'

import useScheduling from './useScheduling'
import { getStartsStops } from './getStartsStops'
import { StartStop } from './types'

//   TODO if user started mining manually, but then we come to a schedule edge that says this type of mining should be stopped - do we stop?
//   TODO and the other way round, if the user stopped mining during schedule, but time passes and another schedule says it should be started - do we start?
const useMiningScheduling = ({
  schedules,
  startMining,
  stopMining,
  getNow = () => new Date(),
}: {
  schedules: Schedule[]
  startMining: (miningType: MiningNodeType) => void
  stopMining: (miningType: MiningNodeType) => void
  getNow?: () => Date
}) => {
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>()
  const [startStops, setStartStops] = useState<StartStop[]>(() => {
    const from = getNow()
    const to = new Date(from.getTime() + 24 * 60 * 60 * 1000)
    return getStartsStops({
      from,
      to,
      schedules,
    })
  })

  useEffect(() => {
    timerRef.current = setTimeout(() => {
      clearTimeout(timerRef.current!)
      const from = getNow()
      const to = new Date(from.getTime() + 24 * 60 * 60 * 1000)
      const startStops = getStartsStops({
        from,
        to,
        schedules,
      })
      setStartStops(startStops)
    }, 24 * 60 * 60 * 1000)

    return () => {
      clearTimeout(timerRef.current!)
    }
  }, [startStops, getNow])

  const scheduledCallback = useCallback(
    (now: Date) => {
      const starts = startStops.filter(
        ss =>
          startOfMinute(ss.start).getTime() === startOfMinute(now).getTime(),
      )
      const stops = startStops.filter(
        ss => startOfMinute(ss.stop).getTime() === startOfMinute(now).getTime(),
      )

      starts.forEach(start => startMining(start.toMine))
      stops.forEach(stop => stopMining(stop.toMine))
    },
    [startStops, startMining, stopMining],
  )

  useScheduling({
    getNow,
    callback: scheduledCallback,
  })
}

export default useMiningScheduling
