import { useAppDispatch } from '../../store/hooks'
import { tbotactions } from '../../store/tbot'
import MessagesConfig from '../../config/helpMessagesConfig'
import IconButton from '../../components/IconButton'
import SvgInfo1 from '../../styles/Icons/Info1'

const AvailableBalanceHelp = () => {
  const dispatch = useAppDispatch()
  const showAvailableBalanceHelp = () =>
    dispatch(tbotactions.push(MessagesConfig.WalletBalanceHelp))

  return (
    <IconButton onClick={showAvailableBalanceHelp}>
      <SvgInfo1 height={20} width='auto' />
    </IconButton>
  )
}

export default AvailableBalanceHelp
