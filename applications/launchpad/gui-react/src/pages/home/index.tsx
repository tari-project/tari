import { useCallback, useRef } from 'react'

import MainLayout from '../../layouts/MainLayout'
import { MiningNodeType } from '../../types/general'
import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { selectSchedules } from '../../store/app/selectors'
import { actions as miningActions } from '../../store/mining'
import useMiningScheduling from '../../useMiningScheduling'

/**
 * Home page: '/'
 */
const HomePage = () => {
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
    (node: MiningNodeType) => console.log('stopping mining', node),
    [],
  )

  useMiningScheduling({
    schedules,
    startMining,
    stopMining,
  })

  return <MainLayout />
}

export default HomePage
