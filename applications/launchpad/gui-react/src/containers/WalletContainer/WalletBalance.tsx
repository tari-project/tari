import { useTheme } from 'styled-components'

import Box from '../../components/Box'
import CoinsList from '../../components/CoinsList'
import Text from '../../components/Text'
import Loading from '../../components/Loading'
import t from '../../locales'

import Chart from './ChartLight'
import AvailableBalanceHelp from './AvailableBalanceHelp'

import { TariSignet, TariAmountContainer } from './styles'
import { useAppSelector } from '../../store/hooks'
import { selectTheme } from '../../store/app/selectors'
import ChartDark from './ChartDark'

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
  const currentTheme = useAppSelector(selectTheme)

  return (
    <Box
      style={{
        background: theme.nodeBackground,
        borderColor: theme.balanceBoxBorder,
        boxShadow: theme.shadow40,
      }}
    >
      <Text color={theme.nodeWarningText}>
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
            color={pending ? theme.placeholderText : theme.helpTipText}
          />
        </div>
        {currentTheme === 'light' ? <Chart /> : <ChartDark />}
      </TariAmountContainer>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
        }}
      >
        <div>
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
            color={pending ? theme.placeholderText : theme.helpTipText}
          />
        </div>
        <AvailableBalanceHelp />
      </div>
    </Box>
  )
}

export default WalletBalance
