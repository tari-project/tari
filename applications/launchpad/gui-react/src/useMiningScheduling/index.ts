import { useCallback, useRef } from 'react'

import { MiningNodeType, ScheduleId } from '../types/general'
import { useAppSelector, useAppDispatch } from '../store/hooks'
import { selectSchedules } from '../store/app/selectors'
import { actions as miningActions } from '../store/mining'

import useMiningScheduling from './useMiningScheduling'
import { MiningActionReason } from '../store/mining/types'
import { useWithWalletPassword } from '../useWithWalletPassword'

/**
 * @name useMiningSchedulingContainer
 * @description connects mining scheduling to the store
 */
const useMiningSchedulingContainer = () => {
  const dispatch = useAppDispatch()
  const schedules = useAppSelector(selectSchedules)
  const startPending = useRef<boolean>(false)
  const startMining = useCallback(
    async (node: MiningNodeType, schedule: ScheduleId) => {
      if (startPending.current) {
        return
      }

      try {
        startPending.current = true
        await dispatch(
          miningActions.startMiningNode({
            node,
            reason: MiningActionReason.Schedule,
            schedule,
          }),
        ).unwrap()
        startPending.current = false
      } finally {
        startPending.current = false
      }
    },
    [],
  )
  const startMiningWithPasswordPrompt = useWithWalletPassword(startMining)

  const stopMining = useCallback(
    (node: MiningNodeType) =>
      dispatch(
        miningActions.stopMiningNode({
          node,
          reason: MiningActionReason.Schedule,
        }),
      ),
    [],
  )

  useMiningScheduling({
    schedules,
    startMining: startMiningWithPasswordPrompt,
    stopMining,
  })
}

export default useMiningSchedulingContainer
