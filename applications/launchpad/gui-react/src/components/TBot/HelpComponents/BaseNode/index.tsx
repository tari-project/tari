import t from '../../../../locales'
import Text from '../../../Text'
import GotItButton from '../GotItButton'
import { StyledTextContainer } from '../styles'

export const WhatIsBaseNode = () => {
  return (
    <>
      <StyledTextContainer>
        <Text type='defaultMedium' as='span'>
          {t.baseNode.helpMessages.howItWorks.allowsYou}
          <ul style={{ margin: 0 }}>
            {t.baseNode.helpMessages.howItWorks.affordances.map(a => (
              <li key={a}>{a}</li>
            ))}
          </ul>
        </Text>
      </StyledTextContainer>
      <StyledTextContainer>
        <div>
          <Text type='defaultHeavy' as='span'>
            {t.baseNode.helpMessages.howItWorks.thankYou}
          </Text>{' '}
          <Text type='defaultMedium' as='span'>
            {t.baseNode.helpMessages.howItWorks.yourContribution}
          </Text>
        </div>
      </StyledTextContainer>
      <GotItButton />
    </>
  )
}

export const ConnectAurora = () => {
  return (
    <>
      <StyledTextContainer>
        <Text type='defaultMedium' as='span'>
          {t.baseNode.helpMessages.aurora}
        </Text>
      </StyledTextContainer>
      <GotItButton />
    </>
  )
}
