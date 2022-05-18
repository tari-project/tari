import t from '../../../../locales'
import { StyledTextContainer } from '../styles'
import Text from '../../../Text'
import GotItButton from '../GotItButton'

const Message1 = () => {
  return (
    <>
      <Text type='defaultHeavy' as='span'>
        {t.mergedMiningHelp.message1}
      </Text>
    </>
  )
}

const Message2 = () => {
  return (
    <>
      <StyledTextContainer>
        <Text type='defaultMedium' as='span'>
          {t.mergedMiningHelp.message2}
        </Text>
      </StyledTextContainer>
      <GotItButton />
    </>
  )
}

export { Message1, Message2 }
