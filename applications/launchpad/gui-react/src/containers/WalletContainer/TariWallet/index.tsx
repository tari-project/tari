import { useTheme } from 'styled-components'

import Box from '../../../components/Box'
import Text from '../../../components/Text'
import Tag from '../../../components/Tag'
import t from '../../../locales'
import { TariBackgroundSignet } from '../styles'

import TariId from './TariId'

const TariWallet = ({
  address,
  emojiId,
  running,
}: {
  address: string
  emojiId: string
  running: boolean
}) => {
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
      {running && (
        <Tag type='running'>
          <Text type='smallHeavy'>{t.common.adjectives.running}</Text>
        </Tag>
      )}
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
      <TariId tariId={address} emojiTariId={emojiId} />
    </Box>
  )
}

export default TariWallet
