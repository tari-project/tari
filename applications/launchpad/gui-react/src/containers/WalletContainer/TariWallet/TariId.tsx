import { useState, useMemo } from 'react'
import { useTheme } from 'styled-components'

import t from '../../../locales'

import Text from '../../../components/Text'
import CopyBox from '../../../components/CopyBox'

import Smiley from './Smiley'
import { TariIdContainer } from './styles'
import Button from '../../../components/Button'
import SvgInfo1 from '../../../styles/Icons/Info1'
import { tbotactions } from '../../../store/tbot'
import MessagesConfig from '../../../config/helpMessagesConfig'
import { useAppDispatch } from '../../../store/hooks'

const SEPARATOR = ' | '

const removeSeparators = (v: string) => v.replaceAll(SEPARATOR, '')

const TariId = ({
  tariId,
  emojiTariId,
}: {
  tariId: string
  emojiTariId: string
}) => {
  const [showEmoji, setShowEmoji] = useState(false)
  const theme = useTheme()
  const dispatch = useAppDispatch()

  const displayedEmojiTariId = useMemo(() => {
    const emojis = Array.from(emojiTariId)
    const emojiChunks = []
    for (let i = 0; i < emojis.length; i += 3) {
      emojiChunks.push(emojis.slice(i, i + 3).join(''))
    }

    return emojiChunks.join(SEPARATOR)
  }, [emojiTariId])

  return (
    <>
      <div style={{ display: 'flex', alignItems: 'center' }}>
        <Text
          as='label'
          color={theme.baseNodeRunningLabel}
          style={{
            display: 'inline-block',
          }}
        >
          {t.wallet.wallet.walletId}{' '}
          <Text as='span' color={theme.textSecondary}>
            ({t.wallet.wallet.address})
          </Text>
        </Text>
        <Button
          variant='button-in-text'
          onClick={() =>
            dispatch(tbotactions.push(MessagesConfig.WalletIdHelp))
          }
          style={{
            color: theme.textSecondary,
            marginLeft: theme.spacingHorizontal(0.4),
            marginBottom: theme.spacingVertical(0.25),
          }}
        >
          <SvgInfo1 fontSize={20} />
        </Button>
      </div>
      <TariIdContainer>
        <CopyBox
          valueTransform={showEmoji ? removeSeparators : undefined}
          value={showEmoji ? displayedEmojiTariId : tariId}
          style={{
            maxWidth: 'calc(100% - 2.4em)',
            borderColor: theme.walletCopyBoxBorder,
            backgroundColor: theme.resetBackground,
            color: theme.textSecondary,
          }}
        />
        <div
          onClick={() => setShowEmoji(a => !a)}
          style={{
            cursor: 'pointer',
            textAlign: 'center',
            position: 'relative',
            top: '0.4em',
          }}
        >
          <Smiley
            on={showEmoji}
            style={{ color: theme.textSecondary, display: 'inline-block' }}
          />
        </div>
      </TariIdContainer>
    </>
  )
}

export default TariId
