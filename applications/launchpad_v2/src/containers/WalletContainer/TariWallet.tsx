import { useState } from 'react'
import styled, { useTheme } from 'styled-components'

import Box from '../../components/Box'
import Text from '../../components/Text'
import Tag from '../../components/Tag'
import t from '../../locales'

import { TariBackgroundSignet } from './styles'
import Smiley from './Smiley'

export const Label = styled.label`
  font-size: 1em;
  display: inline-block;
  margin-bottom: ${({ theme }) => theme.spacingVertical()};
  color: ${({ theme }) => theme.inverted.primary};
`

export const SemiTransparent = styled.span`
  opacity: 0.7;
`

const TariId = ({ tariId }: { tariId: string }) => {
  const [showEmoji, setShowEmoji] = useState(false)
  const theme = useTheme()

  return (
    <>
      <Label>
        Tari Wallet ID <SemiTransparent>(address)</SemiTransparent>
      </Label>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          columnGap: theme.spacing(0.5),
        }}
      >
        <div
          style={{
            flexGrow: 1,
            border: `1px solid ${theme.borderColor}`,
            borderRadius: theme.tightBorderRadius(),
            padding: `${theme.spacingVertical()} ${theme.spacingHorizontal(
              0.75,
            )}`,
            color: theme.borderColor,
            backgroundColor: theme.resetBackground,
          }}
        >
          {showEmoji ? 'emojis here' : tariId}
        </div>
        <div
          onClick={() => setShowEmoji(a => !a)}
          style={{ cursor: 'pointer', width: 25, textAlign: 'center' }}
        >
          <Smiley
            on={showEmoji}
            style={{ color: theme.borderColor, display: 'inline-block' }}
          />
        </div>
      </div>
    </>
  )
}

const TariWallet = ({ address }: { address: string }) => {
  const theme = useTheme()

  return (
    <Box
      border={false}
      style={{
        background: theme.tariGradient,
        position: 'relative',
      }}
    >
      <TariBackgroundSignet style={{ color: theme.accentDark }} />
      <Tag type='running'>
        <Text type='smallMedium'>{t.common.adjectives.running}</Text>
      </Tag>
      <Text
        type='header'
        style={{
          marginBottom: theme.spacing(),
          color: theme.inverted.primary,
          marginTop: theme.spacing(0.5),
        }}
      >
        {t.wallet.wallet.title}
      </Text>
      <TariId tariId={address} />
    </Box>
  )
}

export default TariWallet
