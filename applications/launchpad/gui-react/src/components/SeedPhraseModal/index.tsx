import { invoke } from '@tauri-apps/api'
import { useEffect, useState } from 'react'
import Alert from '../Alert'
import Modal from '../Modal'
import t from '../../locales'
import ConfirmPhrasePage from './ConfirmPhrasePage'
import IntroPage from './IntroPage'
import WordsPage from './WordsPage'
import Text from '../Text'
import { BottomBar, Content, CenteredContent, ModalContent } from './styles'
import Button from '../Button'

const SeedPhraseModal = ({
  open,
  setOpen,
}: {
  open: boolean
  setOpen: (status: boolean) => void
}) => {
  const [page, setPage] = useState(0)
  const [loading, setLoading] = useState(true)
  const [phrase, setPhrase] = useState<string[] | undefined>()
  const [error, setError] = useState<string | undefined>(undefined)

  const onCancel = () => {
    setOpen(false)
  }

  useEffect(() => {
    const getSeedWords = async () => {
      try {
        const seedWords: string[] = await invoke('get_seed_words')
        setPhrase(seedWords)
      } catch (err) {
        setError((err as Error).toString())
      } finally {
        setLoading(false)
      }
    }

    if (open && (!phrase || phrase.length === 0)) {
      getSeedWords()
    }
  }, [open])

  useEffect(() => {
    if (error) {
      setOpen(false)
    }
  }, [error])

  const renderPage = () => {
    if (loading) {
      return (
        <ModalContent>
          <Content>
            <CenteredContent>
              <Text>{t.common.adjectives.loading}...</Text>
            </CenteredContent>
          </Content>
          <BottomBar>
            <Button
              variant='secondary'
              onClick={() => setOpen(false)}
              fullWidth
            >
              {t.common.verbs.cancel}
            </Button>
          </BottomBar>
        </ModalContent>
      )
    }
    if (!phrase) {
      return (
        <ModalContent>
          <Content>
            <CenteredContent>
              <Text>{t.settings.security.couldNotGetSeedWords}</Text>
            </CenteredContent>
          </Content>
          <BottomBar>
            <Button onClick={() => setOpen(false)} fullWidth>
              {t.common.verbs.close}
            </Button>
          </BottomBar>
        </ModalContent>
      )
    }

    switch (page) {
      case 1:
      case 2:
      case 3:
      case 4:
      case 5:
      case 6:
        return (
          <WordsPage
            words={phrase.slice((page - 1) * 4, page * 4)}
            startingNumber={page}
            onPrevPage={() => setPage(p => p - 1)}
            onNextPage={() => setPage(p => p + 1)}
          />
        )

      case 7:
        return (
          <ConfirmPhrasePage
            phrase={phrase}
            onBack={() => setPage(p => p - 1)}
            onSuccess={() => setOpen(false)}
            onError={err => setError(err)}
          />
        )

      default:
        return (
          <IntroPage
            phrase={phrase}
            onCancel={onCancel}
            onSubmit={() => setPage(1)}
          />
        )
    }
  }

  return (
    <>
      <Modal open={open} size='small' style={{ width: 480 }}>
        {renderPage()}
      </Modal>
      <Alert
        open={Boolean(error)}
        content={error}
        onClose={() => setError(undefined)}
      />
    </>
  )
}

export default SeedPhraseModal
