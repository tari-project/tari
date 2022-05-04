import { useTheme } from 'styled-components'

import Box from '../../components/Box'
import Text from '../../components/Text'
import * as FormatUtils from '../../utils/Format'
import t from '../../locales'

import Chart from './Chart'

import { TariSignet, TariAmountContainer } from './styles'

const WalletBalance = ({
  balance,
  available,
}: {
  balance: number
  available: number
}) => {
  const theme = useTheme()

  return (
    <Box>
      <Text color={theme.secondary}>{t.wallet.balance.title}</Text>
      <TariAmountContainer>
        <Text type='header'>
          <TariSignet
            style={{
              color: theme.accent,
              display: 'inline-block',
              marginRight: theme.spacingHorizontal(0.5),
            }}
          />
          {FormatUtils.amount(balance)}
        </Text>
        <Chart />
      </TariAmountContainer>
      <Text
        type='defaultMedium'
        style={{ display: 'inline-block' }}
        color={theme.secondary}
      >
        {t.wallet.balance.available}
      </Text>{' '}
      <Text type='defaultHeavy' style={{ display: 'inline-block' }}>
        {FormatUtils.amount(available)}
      </Text>
    </Box>
  )
}

export default WalletBalance
