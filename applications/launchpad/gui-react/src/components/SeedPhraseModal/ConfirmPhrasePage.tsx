import { useEffect, useState } from 'react'

import Button from '../Button'
import Input from '../Inputs/Input'
import Text from '../Text'

import t from '../../locales'

import { useAppDispatch } from '../../store/hooks'
import { actions as walletActions } from '../../store/wallet'

import {
  BottomBar,
  Content,
  ErrorContainer,
  ErrorText,
  ModalContent,
  NumWrapper,
  TextSection,
  WordInputRow,
  WordsContainer,
} from './styles'
import { invoke } from '@tauri-apps/api'

const ConfirmPhrasePage = ({
  phrase,
  onSuccess,
  onBack,
  onError,
}: {
  phrase: string[]
  onSuccess: () => void
  onBack: () => void
  onError: (err: string) => void
}) => {
  const dispatch = useAppDispatch()

  const [checkWords, setCheckWords] = useState<number[]>([3, 10, 14, 21])
  const [enteredWords, setEnteredWords] = useState<Record<number, string>>({})
  const [error, setError] = useState(false)

  const drawWords = (ws: number[]) => {
    const rand = Math.floor(Math.random() * phrase.length)
    if (ws.length === 4) {
      setCheckWords(ws)
      return
    }

    if (ws.includes(rand)) {
      drawWords(ws)
    } else {
      drawWords(ws.concat([rand]))
    }
  }

  useEffect(() => {
    setError(false)
    drawWords([])
  }, [phrase])

  const onSubmit = async () => {
    const anyInvalid = checkWords.find(
      i => !enteredWords[i] || phrase[i] !== enteredWords[i],
    )

    if (anyInvalid) {
      setError(true)
    } else {
      setError(false)
      dispatch(walletActions.setRecoveryPhraseAsCreated())
      onSuccess()

      try {
        await invoke('delete_seed_words')
      } catch (err) {
        onError((err as Error).toString())
      }
    }
  }

  return (
    <ModalContent>
      <Content>
        <Text as='h2' type='subheader'>
          {t.settings.security.backupRecoveryPhrase}
        </Text>
        <TextSection>
          <Text type='smallMedium'>
            {t.settings.security.confirmPhraseDesc}
          </Text>
        </TextSection>
        <WordsContainer>
          {checkWords.map((w, idx) => (
            <WordInputRow key={idx}>
              <NumWrapper>
                <Text>{w + 1}.</Text>
              </NumWrapper>
              <Input
                onChange={val => {
                  setEnteredWords(st => ({ ...st, [w]: val }))
                }}
                withError={false}
              />
            </WordInputRow>
          ))}
        </WordsContainer>
      </Content>
      <BottomBar>
        <Button variant='secondary' onClick={onBack}>
          <Text type='smallHeavy'>
            {t.settings.security.backToRecoveryPhrase}
          </Text>
        </Button>
        <Button onClick={onSubmit}>
          <Text type='smallHeavy'>{t.settings.security.submitAndFinish}</Text>
        </Button>
      </BottomBar>
      {error && (
        <ErrorContainer>
          <ErrorText>
            <Text type='smallMedium'>
              {t.settings.security.phraseConfirmError}
            </Text>
          </ErrorText>
          <BottomBar>
            <Button variant='secondary' onClick={onBack}>
              <Text type='smallHeavy'>
                {t.settings.security.backToRecoveryPhrase}
              </Text>
            </Button>
            <Button onClick={() => setError(false)}>
              <Text type='smallHeavy'>{t.common.verbs.tryAgain}</Text>
            </Button>
          </BottomBar>
        </ErrorContainer>
      )}
    </ModalContent>
  )
}

export default ConfirmPhrasePage
