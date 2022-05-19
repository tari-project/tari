import { tbotactions } from './../store/tbot/index'
import { useAppDispatch } from '../store/hooks'

export const TBotClose = () => {
  const dispatch = useAppDispatch()
  return dispatch(tbotactions.close())
}
