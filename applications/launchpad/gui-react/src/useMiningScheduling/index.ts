import { useCallback, useRef } from 'react'

import { MiningNodeType } from '../types/general'
import { useAppSelector, useAppDispatch } from '../store/hooks'
import { selectSchedules } from '../store/app/selectors'
import { actions as miningActions } from '../store/mining'

import useMiningScheduling from './useMiningScheduling'

/**
 * @name useMiningSchedulingContainer
 * @description connects mining scheduling to the store
 *
 */
const useMiningSchedulingContainer = () => {
  const dispatch = useAppDispatch()
  const schedules = useAppSelector(selectSchedules)
  const startPending = useRef<boolean>(false)
  const startMining = useCallback(async (node: MiningNodeType) => {
    if (startPending.current) {
      return
    }

    try {
      startPending.current = true
      await dispatch(miningActions.startMiningNode({ node })).unwrap()
      startPending.current = false
    } finally {
      startPending.current = false
    }
  }, [])
  const stopMining = useCallback(
    (node: MiningNodeType) => dispatch(miningActions.stopMiningNode({ node })),
    [],
  )

  useMiningScheduling({
    schedules,
    startMining,
    stopMining,
  })
}

export default useMiningSchedulingContainer
