import styled, { useTheme } from 'styled-components'

import Box from '../../components/Box'
import Text from '../../components/Text'
import Button from '../../components/Button'
import * as FormatUtils from '../../utils/Format'
import Arrow from '../../styles/Icons/ArrowTop2'
import t from '../../locales'

import Chart from './Chart'

import { TariSignet, TariAmountContainer } from './styles'

const StyledArrow = styled(Arrow)`
  transform: rotate(45deg);
  margin-top: -0.4em;
  width: 2em;
  height: 2em;
`

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
      <Text>{t.wallet.balance.title}</Text>
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
      <Text type='defaultMedium' style={{ display: 'inline-block' }}>
        {t.wallet.balance.available}
      </Text>{' '}
      <Text type='defaultHeavy' style={{ display: 'inline-block' }}>
        {FormatUtils.amount(available)}
      </Text>
      <Button
        rightIcon={<StyledArrow />}
        style={{
          paddingRight: theme.spacingHorizontal(1.5),
          marginTop: theme.spacingVertical(1),
        }}
      >
        <Text
          type='defaultMedium'
          style={{
            lineHeight: '100%',
            display: 'flex',
            alignItems: 'center',
          }}
        >
          {t.wallet.balance.sendCta}
        </Text>
      </Button>
    </Box>
  )
}

export default WalletBalance
