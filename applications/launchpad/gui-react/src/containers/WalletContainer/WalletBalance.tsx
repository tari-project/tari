import { useTheme } from 'styled-components'

import Box from '../../components/Box'
import CoinsList from '../../components/CoinsList'
import Text from '../../components/Text'
import Loading from '../../components/Loading'
import t from '../../locales'

import Chart from './Chart'

import { TariSignet, TariAmountContainer } from './styles'

const WalletBalance = ({
  balance,
  available,
  pending,
}: {
  balance: number
  available: number
  pending: boolean
}) => {
  const theme = useTheme()

  return (
    <Box>
      <Text color={theme.secondary}>
        {t.wallet.balance.title}
        <Loading loading={pending} size='0.9em' style={{ marginLeft: '5px' }} />
      </Text>
      <TariAmountContainer>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
          }}
        >
          <TariSignet
            style={{
              color: theme.accent,
              display: 'inline-block',
              marginRight: theme.spacingHorizontal(0.5),
            }}
          />
          <CoinsList
            coins={[{ amount: balance, unit: 'xtr' }]}
            inline
            color={pending ? theme.placeholderText : 'inherit'}
          />
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
      <CoinsList
        coins={[{ amount: available, unit: 'xtr' }]}
        inline
        small
        color={pending ? theme.placeholderText : 'inherit'}
      />
    </Box>
  )
}

export default WalletBalance
