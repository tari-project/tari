import { useTheme } from 'styled-components'

import Box from '../../components/Box'
import Text from '../../components/Text'
import Button from '../../components/Button'
import t from '../../locales'

import { TariBackgroundSignet } from './styles'

const PasswordBox = ({
  pending,
  onSubmit,
}: {
  pending: boolean
  onSubmit: (password: string) => void
}) => {
  const theme = useTheme()
  const password = 'placeholderPassword'

  return (
    <Box style={{ position: 'relative' }}>
      <TariBackgroundSignet />
      <Text type='header' style={{ marginBottom: theme.spacing() }}>
        {t.wallet.password.title}
      </Text>
      <Text>{t.wallet.password.cta}</Text>
      <Box border={false} style={{ padding: 0 }}>
        placeholder for input
      </Box>
      <Button
        disabled={pending}
        loading={pending}
        onClick={() => onSubmit(password)}
      >
        <Text type='defaultMedium' style={{ lineHeight: '100%' }}>
          {t.common.verbs.continue}
        </Text>
      </Button>
    </Box>
  )
}

export default PasswordBox
