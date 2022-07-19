import { useEffect } from 'react'

import { useAppSelector, useAppDispatch } from '../../../store/hooks'
import { selectTariBalance } from '../../../store/wallet/selectors'
import { actions } from '../../../store/wallet'

const useWalletBalance = () => {
  const dispatch = useAppDispatch()
  useEffect(() => {
    dispatch(actions.updateWalletBalance())
  }, [])

  const balance = useAppSelector(selectTariBalance)
  return balance
}

export default useWalletBalance
