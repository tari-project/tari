import Button from '../Button'
import Text from '../Text'

import t from '../../locales'

import {
  BottomBar,
  Content,
  ModalContent,
  TextSection,
  WordDisplay,
  WordsContainer,
} from './styles'

const WordsPage = ({
  words,
  startingNumber,
  onPrevPage,
  onNextPage,
}: {
  words: string[]
  startingNumber: number
  onPrevPage: () => void
  onNextPage: () => void
}) => {
  return (
    <ModalContent>
      <Content>
        <Text as='h2' type='subheader'>
          {t.settings.security.backupRecoveryPhrase}
        </Text>
        <TextSection>
          <Text type='smallMedium'>
            🖌 {t.settings.security.writeDownRecoveryPhraseInstructions}
          </Text>
        </TextSection>
        <WordsContainer>
          {words.map((word, idx) => (
            <WordDisplay key={idx}>
              <Text>
                {(startingNumber - 1) * 4 + idx + 1}. {word}
              </Text>
            </WordDisplay>
          ))}
        </WordsContainer>
      </Content>
      <BottomBar>
        <Button variant='secondary' onClick={onPrevPage}>
          <Text type='smallHeavy'> {t.settings.security.prev4Words}</Text>
        </Button>
        <Button onClick={onNextPage}>
          <Text type='smallHeavy'>{t.settings.security.next4Words}</Text>
        </Button>
      </BottomBar>
    </ModalContent>
  )
}

export default WordsPage
