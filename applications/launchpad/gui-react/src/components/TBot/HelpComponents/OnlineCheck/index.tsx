import t from '../../../../locales'
import { StyledTextContainer, ListItem, ListGroup } from '../styles'
import Text from '../../../Text'
import GotItButton from '../GotItButton'

export const LooksLikeYoureOffline = () => (
  <StyledTextContainer style={{ flexDirection: 'column' }}>
    <Text type='defaultHeavy' as='span'>
      {t.online.youreOffline}
    </Text>
    <Text type='defaultMedium'>{t.online.noInternet}</Text>
  </StyledTextContainer>
)

export const ReconnectToInternet = () => (
  <>
    <StyledTextContainer style={{ flexDirection: 'column' }}>
      <Text type='defaultMedium' as='span'>
        {t.online.tryThose}
      </Text>
      <ListGroup>
        <ListItem>{t.online.checkRouter}</ListItem>
        <ListItem>{t.online.resetRouter}</ListItem>
        <ListItem>{t.online.reconnectWifi}</ListItem>
      </ListGroup>
    </StyledTextContainer>
    <GotItButton />
  </>
)
