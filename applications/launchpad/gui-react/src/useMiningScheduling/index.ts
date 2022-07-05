import { useCallback, useRef } from 'react'

import { MiningNodeType, ScheduleId } from '../types/general'
import { useAppSelector, useAppDispatch } from '../store/hooks'
import { selectSchedules } from '../store/app/selectors'
import { actions as miningActions } from '../store/mining'
import { MiningActionReason } from '../store/mining/types'
import useWithPasswordPrompt from '../containers/PasswordPrompt/useWithPasswordPrompt'

import useMiningScheduling from './useMiningScheduling'

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
  const startMiningWithPasswordPrompt = useWithPasswordPrompt(startMining, {
    wallet: true,
    monero: node => node === 'merged',
  })

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
