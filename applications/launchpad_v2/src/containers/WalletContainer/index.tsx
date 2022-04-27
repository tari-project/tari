import { useTheme } from 'styled-components'
import Box from '../../components/Box'
import Text from '../../components/Text'
import Button from '../../components/Button'
import Loading from '../../components/Loading'

import { Container, TariSignet } from './styles'

const WalletContainer = () => {
  const theme = useTheme()
  const disabled = false
  const loading = false

  return (
    <Container>
      <Box style={{ position: 'relative' }}>
        <TariSignet />
        <Text type='header' style={{ marginBottom: theme.spacing() }}>
          Enter Password
        </Text>
        <Text>to access your wallet:</Text>
        <Box border={false} style={{ padding: 0 }}>
          placeholder for input
        </Box>
        <Button
          disabled={disabled}
          variant={disabled ? 'disabled' : undefined}
          rightIcon={<Loading loading={loading} />}
        >
          <Text type='defaultMedium' style={{ lineHeight: '100%' }}>
            Continue
          </Text>
        </Button>
      </Box>
    </Container>
  )
}

export default WalletContainer
