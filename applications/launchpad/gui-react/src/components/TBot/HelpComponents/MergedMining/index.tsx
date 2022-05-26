import t from '../../../../locales'
import { StyledTextContainer } from '../styles'
import Text from '../../../Text'
import GotItButton from '../GotItButton'

import { useAppDispatch } from '../../../../store/hooks'
import { tbotactions } from '../../../../store/tbot'

const Message1 = () => {
  return (
    <>
      <Text type='defaultHeavy' as='span' testId='message1-cmp'>
        {t.mergedMiningHelp.message1}
      </Text>
    </>
  )
}

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

const Message3 = () => {
  return (
    <>
      <Text type='defaultHeavy' as='span' testId='message1-cmp'>
        {t.mergedMiningHelp.message3}
      </Text>
    </>
  )
}
const Message4 = () => {
  return (
    <>
      <Text type='defaultHeavy' as='span' testId='message1-cmp'>
        {t.mergedMiningHelp.message4}
      </Text>
    </>
  )
}
const Message5 = () => {
  return (
    <>
      <Text type='defaultHeavy' as='span' testId='message1-cmp'>
        {t.mergedMiningHelp.message5}
      </Text>
    </>
  )
}
const Message6 = () => {
  return (
    <>
      <Text type='defaultHeavy' as='span' testId='message1-cmp'>
        {t.mergedMiningHelp.message6}
      </Text>
    </>
  )
}

export { Message1, Message2, Message3, Message4, Message5, Message6 }
