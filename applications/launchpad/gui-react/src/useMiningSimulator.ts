import { useEffect } from 'react'

import { store } from './store'
import { useAppDispatch } from './store/hooks'
import { actions as miningActions } from './store/mining'

function delay(n: number) {
  return new Promise(function (resolve) {
    setTimeout(resolve, n * 1000)
  })
}

/**
 * @TODO - remove after mining dev
 */
const useMiningSimulator = () => {
  const dispatch = useAppDispatch()

  useEffect(() => {
    const timer = setInterval(async () => {
      const sessions = store.getState().mining.tari.sessions
      if (!sessions || sessions[sessions.length - 1].finishedAt) {
        return
      }
      const sessionId = sessions[sessions.length - 1].id
      dispatch(
        miningActions.setPendingInSession({
          node: 'tari',
          active: true,
          sessionId,
        }),
      )
      await delay(1)
      dispatch(miningActions.addAmount({ amount: '1000', node: 'tari' }))
      dispatch(
        miningActions.setPendingInSession({
          node: 'tari',
          active: false,
          sessionId,
        }),
      )
    }, 5e3)
    return () => clearInterval(timer)
  }, [])

  useEffect(() => {
    const timer = setInterval(async () => {
      const sessions = store.getState().mining.merged.sessions
      if (!sessions || sessions[sessions.length - 1].finishedAt) {
        return
      }
      const sessionId = sessions[sessions.length - 1].id
      dispatch(
        miningActions.setPendingInSession({
          node: 'merged',
          active: true,
          sessionId,
        }),
      )
      await delay(1)
      dispatch(miningActions.addAmount({ amount: '50', node: 'merged' }))
      dispatch(
        miningActions.setPendingInSession({
          node: 'merged',
          active: false,
          sessionId,
        }),
      )
    }, 7e3)
    return () => clearInterval(timer)
  }, [])
}

export default useMiningSimulator
