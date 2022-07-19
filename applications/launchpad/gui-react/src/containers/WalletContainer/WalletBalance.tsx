import { useTheme } from 'styled-components'

import Box from '../../components/Box'
import CoinsList from '../../components/CoinsList'
import Text from '../../components/Text'
import Loading from '../../components/Loading'
import t from '../../locales'

import Chart from './ChartLight'
import AvailableBalanceHelp from './AvailableBalanceHelp'

import { useAppSelector } from '../../store/hooks'
import { selectTheme } from '../../store/app/selectors'
import ChartDark from './ChartDark'

import {
  TariSignet,
  TariAmountContainer,
  BoxTopContainer,
  BoxBottomContainer,
} from './styles'
import Button from '../../components/Button'
import SvgArrowRight from '../../styles/Icons/ArrowRight'
import SendModal from './Send/SendModal'
import { useState } from 'react'

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

  const [showSendModal, setShowSendModal] = useState(false)

  return (
    <Box
      style={{
        background: theme.nodeBackground,
        borderColor: theme.balanceBoxBorder,
        boxShadow: theme.shadow40,
        padding: 0,
      }}
    >
      <BoxTopContainer>
        <Text color={theme.nodeWarningText}>
          {t.wallet.balance.title}
          <Loading
            loading={pending}
            size='0.9em'
            style={{ marginLeft: '5px' }}
          />
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
      </BoxTopContainer>
      <BoxBottomContainer>
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
        {available && available > 0 ? (
          <Button
            onClick={() => setShowSendModal(true)}
            style={{
              marginTop: theme.spacingVertical(0.5),
            }}
            disabled={pending}
            rightIcon={
              <SvgArrowRight
                style={{
                  transform: 'rotate(-45deg)',
                }}
              />
            }
          >
            {t.wallet.balance.sendCta}
          </Button>
        ) : null}
      </BoxBottomContainer>

      <SendModal
        open={showSendModal}
        onClose={() => setShowSendModal(false)}
        available={available}
      />
    </Box>
  )
}

export default WalletBalance
