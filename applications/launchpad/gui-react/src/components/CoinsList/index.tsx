import Loading from '../Loading'
import Text from '../Text'

import { CoinsListItem, StyledCoinsList } from './styles'
import { CoinsListProps } from './types'

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
const CoinsList = ({ coins, color }: CoinsListProps) => {
  return (
    <StyledCoinsList color={color}>
      {coins.map((c, idx) => (
        <CoinsListItem key={`coin-${idx}`} $loading={c.loading}>
          {c.loading ? (
            <Loading loading={true} style={{ marginRight: 12 }} />
          ) : null}
          <Text type='subheader'>{c.amount}</Text>
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
