import { useEffect } from 'react'

import { store } from './store'
import { useAppDispatch } from './store/hooks'
import { actions as miningActions } from './store/mining'

/**
 * @TODO - remove after mining dev
 */
const useMiningSimulator = () => {
  const dispatch = useAppDispatch()

  useEffect(() => {
    const timer = setInterval(async () => {
      const session = store.getState().mining.tari.session
      if (!session || session.finishedAt) {
        return
      }
      dispatch(miningActions.addAmount({ amount: '1000.1232', node: 'tari' }))
    }, 5e3)
    return () => clearInterval(timer)
  }, [])

  useEffect(() => {
    const timer = setInterval(async () => {
      const session = store.getState().mining.merged.session
      if (!session || session.finishedAt) {
        return
      }
      dispatch(miningActions.addAmount({ amount: '50', node: 'merged' }))
    }, 7e3)
    return () => clearInterval(timer)
  }, [])
}

export default useMiningSimulator
