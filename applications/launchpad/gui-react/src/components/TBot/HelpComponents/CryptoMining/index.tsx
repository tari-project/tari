import GotItButton from '../GotItButton'
import t from '../../../../locales'
import { StyledTextContainer } from '../styles'
import Text from '../../../Text'

const Message1 = () => {
  return (
    <>
      <StyledTextContainer>
        <Text type='defaultMedium' as='span' testId='message-cmp'>
          {t.cryptoMiningHelp.message1}
        </Text>
      </StyledTextContainer>
      <GotItButton />
    </>
  )
}

export { Message1 }
