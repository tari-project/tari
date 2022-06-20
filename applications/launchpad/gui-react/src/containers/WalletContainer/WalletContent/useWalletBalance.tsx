import { useEffect } from 'react'

import { useAppSelector, useAppDispatch } from '../../../store/hooks'
import { selectTariBalance } from '../../../store/wallet/selectors'
import { actions } from '../../../store/wallet'

const useWalletBalance = () => {
  const dispatch = useAppDispatch()
  const balance = useAppSelector(selectTariBalance)

  useEffect(() => {
    const { abort } = dispatch(actions.updateWalletBalance())

    return abort
  }, [])

  return balance
}

export default useWalletBalance
