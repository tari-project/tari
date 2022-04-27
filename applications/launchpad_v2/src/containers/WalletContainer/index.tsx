import { useTheme } from 'styled-components'
import Box from '../../components/Box'
import Text from '../../components/Text'
import Button from '../../components/Button'
import Loading from '../../components/Loading'

const WalletContainer = () => {
  const theme = useTheme()
  const disabled = false
  const loading = false

  return (
    <div
      style={{
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'center',
        height: '100%',
      }}
    >
      <Box>
        <Text type='header' style={{ marginBottom: theme.spacing() }}>
          Enter Password
        </Text>
        <Text>to access your wallet:</Text>
        <Box border={false}>placeholder for input</Box>
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
    </div>
  )
}

export default WalletContainer
