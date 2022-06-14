import t from '../../../../locales'
import { StyledTextContainer } from '../styles'
import Text from '../../../Text'
import GotItButton from '../GotItButton'

import { useAppDispatch } from '../../../../store/hooks'
import { tbotactions } from '../../../../store/tbot'

const Message1 = (
  <>
    <Text type='defaultHeavy' as='span' testId='message1-cmp'>
      {t.mergedMiningHelp.message1}
    </Text>
  </>
)

const Message2 = () => {
  const dispatch = useAppDispatch()

  const close = () => {
    return dispatch(tbotactions.close())
  }

  return (
    <>
      <StyledTextContainer>
        <Text type='defaultMedium' as='span' testId='message2-cmp'>
          {t.mergedMiningHelp.message2}
        </Text>
      </StyledTextContainer>
      <GotItButton onClick={close} />
    </>
  )
}

export { Message1, Message2 }
