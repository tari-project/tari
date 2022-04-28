import { useState } from 'react'
import { useTheme } from 'styled-components'

import Smiley from './Smiley'
import { Label, SemiTransparent, TariIdContainer, TariIdBox } from './styles'

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
      <Label>
        Tari Wallet ID <SemiTransparent>(address)</SemiTransparent>
      </Label>
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
