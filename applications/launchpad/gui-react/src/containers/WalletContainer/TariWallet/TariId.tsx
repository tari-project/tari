import { useState } from 'react'
import { useTheme } from 'styled-components'

import t from '../../../locales'

import Text from '../../../components/Text'
import CopyBox from '../../../components/CopyBox'

import Smiley from './Smiley'
import { SemiTransparent, TariIdContainer } from './styles'

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
        <CopyBox
          value={showEmoji ? emojiTariId.join(' | ') : tariId}
          style={{
            maxWidth: 'calc(100% - 2.4em)',
            borderColor: theme.borderColor,
            backgroundColor: theme.resetBackground,
            color: theme.borderColor,
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
            style={{ color: theme.borderColor, display: 'inline-block' }}
          />
        </div>
      </TariIdContainer>
    </>
  )
}

export default TariId
