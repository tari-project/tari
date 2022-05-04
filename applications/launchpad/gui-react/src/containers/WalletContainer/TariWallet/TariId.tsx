import { useState } from 'react'
import { useTheme } from 'styled-components'

import t from '../../../locales'

import Text from '../../../components/Text'

import Smiley from './Smiley'
import { SemiTransparent, TariIdContainer, TariIdBox } from './styles'

const TariId = ({
  tariId,
  emojiTariId,
}: {
  tariId: string
  emojiTariId: string[]
}) => {
  const [showEmoji, setShowEmoji] = useState(false)
  const theme = useTheme()

  return (
    <>
      <Text
        as='label'
        color={theme.inverted.primary}
        style={{
          display: 'inline-block',
          marginBottom: theme.spacingVertical(0.62),
        }}
      >
        {t.wallet.wallet.walletId}{' '}
        <SemiTransparent>({t.wallet.wallet.address})</SemiTransparent>
      </Text>
      <TariIdContainer>
        <TariIdBox>{showEmoji ? emojiTariId.join(' | ') : tariId}</TariIdBox>
        <div
          onClick={() => setShowEmoji(a => !a)}
          style={{
            cursor: 'pointer',
            textAlign: 'center',
          }}
        >
          <Smiley
            on={showEmoji}
            style={{ color: theme.borderColor, display: 'inline-block' }}
          />
        </div>
      </TariIdContainer>
    </>
  )
}

export default TariId
