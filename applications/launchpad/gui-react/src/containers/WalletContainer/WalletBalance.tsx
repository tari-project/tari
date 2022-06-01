import { useTheme } from 'styled-components'

import Box from '../../components/Box'
import CoinsList from '../../components/CoinsList'
import Text from '../../components/Text'
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
        <div style={{ display: 'flex', alignItems: 'center' }}>
          <TariSignet
            style={{
              color: theme.accent,
              display: 'inline-block',
              marginRight: theme.spacingHorizontal(0.5),
            }}
          />
          <CoinsList coins={[{ amount: balance, unit: 'xtr' }]} inline />
        </div>
        <Chart />
      </TariAmountContainer>
      <Text
        type='defaultMedium'
        style={{ display: 'inline-block' }}
        color={theme.secondary}
      >
        {t.wallet.balance.available}
      </Text>{' '}
      <CoinsList coins={[{ amount: available, unit: 'xtr' }]} inline small />
    </Box>
  )
}

export default WalletBalance
