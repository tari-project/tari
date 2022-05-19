import GotItButton from '../GotItButton'
import t from '../../../../locales'
import { StyledTextContainer } from '../styles'
import Text from '../../../Text'
import { TBotClose } from '../../../../utils/TBotHelpers'

const Message1 = () => {
  return (
    <>
      <StyledTextContainer>
        <Text type='defaultMedium' as='span' testId='message-cmp'>
          {t.cryptoMiningHelp.message1}
        </Text>
      </StyledTextContainer>
      <GotItButton onClick={TBotClose} />
    </>
  )
}

export { Message1 }
