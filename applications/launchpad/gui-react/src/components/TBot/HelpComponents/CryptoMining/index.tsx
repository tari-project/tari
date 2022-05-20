import GotItButton from '../GotItButton'
import t from '../../../../locales'
import { StyledTextContainer } from '../styles'
import Text from '../../../Text'
import { useAppDispatch } from '../../../../store/hooks'
import { tbotactions } from '../../../../store/tbot'

const Message1 = () => {
  const dispatch = useAppDispatch()
  const close = () => {
    dispatch(tbotactions.close())
  }
  return (
    <>
      <StyledTextContainer>
        <Text type='defaultMedium' as='span' testId='message-cmp'>
          {t.cryptoMiningHelp.message1}
        </Text>
      </StyledTextContainer>
      <GotItButton onClick={close} />
    </>
  )
}

export { Message1 }
