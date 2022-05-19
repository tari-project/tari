import t from '../../../../locales'
import { StyledTextContainer } from '../styles'
import Text from '../../../Text'
import GotItButton from '../GotItButton'

import { TBotClose } from '../../../../utils/TBotHelpers'

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
  return (
    <>
      <StyledTextContainer>
        <Text type='defaultMedium' as='span' testId='message2-cmp'>
          {t.mergedMiningHelp.message2}
        </Text>
      </StyledTextContainer>
      <GotItButton onClick={TBotClose} />
    </>
  )
}

export { Message1, Message2 }
