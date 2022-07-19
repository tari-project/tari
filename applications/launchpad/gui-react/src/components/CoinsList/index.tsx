import { formatAmount } from '../../utils/Format'
import Loading from '../Loading'
import Text from '../Text'
import { TextType } from '../Text/types'

import { CoinsListItem, IconWrapper, StyledCoinsList } from './styles'
import { CoinsListProps } from './types'

/**
 * Render the list of coins with amount.
 * @param {CoinProps[]} coins - the list of coins
 * @param {string} [color = 'inherit'] - the main text color
 * @param {string} [unitsColor] - color of units text
 * @param {boolean} [inline] - if true, renders as inline block
 * @param {boolean} [small] - if true, renders smaller font sizes
 *
 * @typedef {CoinProps}
 * @param {string | number} amount - the amount
 * @param {string} unit - the unit, ie. xtr
 * @param {string} [suffixText] - the latter text after the amount and unit
 * @param {boolean} [loading] - is value being loaded
 *
 * @example
 * <CoinsList coins={[{ amount: balance.value, unit: balance.currency }]} />
 */
const CoinsList = ({
  coins,
  color,
  unitsColor,
  showSymbols,
  inline,
  small,
}: CoinsListProps) => {
  const textSize: { amount: TextType; suffix: TextType } = small
    ? { amount: 'defaultHeavy', suffix: 'microRegular' }
    : { amount: 'subheader', suffix: 'smallMedium' }

  return (
    <StyledCoinsList color={color} inline={inline}>
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

          <Text type={textSize.amount}>{formatAmount(c.amount)}</Text>
          <Text
            as='span'
            type={textSize.suffix}
            style={{
              paddingLeft: 4,
              paddingRight: 4,
              textTransform: 'uppercase',
              color: unitsColor,
            }}
          >
            {c.unit}
          </Text>
          {c.suffixText ? (
            <Text as='span' type={textSize.suffix}>
              {c.suffixText}
            </Text>
          ) : null}
        </CoinsListItem>
      ))}
    </StyledCoinsList>
  )
}

export default CoinsList
