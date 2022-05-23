import Loading from '../Loading'
import Text from '../Text'

import { CoinsListItem, IconWrapper, StyledCoinsList } from './styles'
import { CoinsListProps } from './types'

const formatAmount = (amount: string) => {
  if (Number(amount) === 0) {
    return '00 000'
  } else {
    // Add spaces to number
    const splitted = amount.toString().split('.')
    splitted[0] = splitted[0].replace(/\B(?=(\d{3})+(?!\d))/g, ' ')
    return splitted.join('.')
  }
}

/**
 * Render the list of coins with amount.
 * @param {CoinProps[]} coins - the list of coins
 * @param {string} [color = 'inherit'] - the text color
 *
 * @typedef {CoinProps}
 * @param {string} amount - the amount
 * @param {string} unit - the unit, ie. xtr
 * @param {string} [suffixText] - the latter text after the amount and unit
 * @param {boolean} [loading] - is value being loaded
 */
const CoinsList = ({ coins, color, showSymbols }: CoinsListProps) => {
  return (
    <StyledCoinsList color={color}>
      {coins.map((c, idx) => (
        <CoinsListItem key={`coin-${idx}`} $loading={c.loading}>
          {c.loading ? (
            <Loading
              loading={true}
              style={{ marginRight: 12, marginTop: -4 }}
            />
          ) : c.icon && showSymbols ? (
            <IconWrapper>{c.icon}</IconWrapper>
          ) : null}

          <Text type='subheader'>{formatAmount(c.amount)}</Text>
          <Text
            as='span'
            type='smallMedium'
            style={{
              paddingLeft: 4,
              paddingRight: 4,
              textTransform: 'uppercase',
            }}
          >
            {c.unit}
          </Text>
          {c.suffixText ? (
            <Text as='span' type='smallMedium'>
              {c.suffixText}
            </Text>
          ) : null}
        </CoinsListItem>
      ))}
    </StyledCoinsList>
  )
}

export default CoinsList
