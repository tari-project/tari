import Button from '../Button'
import Text from '../Text'

import t from '../../locales'

import {
  BottomBar,
  Content,
  ModalContent,
  PrintButtonWrapper,
  TextSection,
} from './styles'
import PrintSheet from './PrintSheet'

export const IntroPage = ({
  phrase,
  onSubmit,
  onCancel,
}: {
  phrase: string[]
  onSubmit: () => void
  onCancel: () => void
}) => {
  return (
    <>
      <ModalContent>
        <Content>
          <Text as='h2' type='subheader'>
            {t.settings.security.backupRecoveryPhrase}
          </Text>
          <TextSection>
            <Text type='smallMedium'>
              📌 {t.settings.security.backupRecoveryPhraseExplanation.part1}
            </Text>
          </TextSection>
          <TextSection>
            <Text type='smallMedium'>
              📌 {t.settings.security.backupRecoveryPhraseExplanation.part2}
            </Text>
          </TextSection>
          <TextSection>
            <Text type='smallMedium'>
              📌 {t.settings.security.backupRecoveryPhraseExplanation.part3}
            </Text>
          </TextSection>
          <PrintButtonWrapper>
            <PrintSheet phrase={phrase} />
          </PrintButtonWrapper>
        </Content>
        <BottomBar>
          <Button variant='secondary' onClick={onCancel}>
            <Text type='smallHeavy'>{t.common.verbs.cancel}</Text>
          </Button>
          <Button onClick={onSubmit} fullWidth>
            <Text type='smallHeavy'>
              {t.settings.security.showRecoveryPhrase}
            </Text>
          </Button>
        </BottomBar>
      </ModalContent>
    </>
  )
}

export default IntroPage
