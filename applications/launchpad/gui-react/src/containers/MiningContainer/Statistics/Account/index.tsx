import { useTheme } from 'styled-components'

import Text from '../../../../components/Text'
import CoinsList from '../../../../components/CoinsList'
import ArrowDown from '../../../../styles/Icons/ArrowBottom2'
import ArrowUp from '../../../../styles/Icons/ArrowTop2'
import t from '../../../../locales'
import { AccountData } from '../types'

/**
 * @name Account
 * @description presentation component showing a row of multiple coin balances together with percentage changes comparing period to period
 *
 * @prop {AccountData} data - data of the account
 */
const Account = ({ data }: { data: AccountData }) => {
  const theme = useTheme()

  return (
    <div
      style={{
        display: 'flex',
        columnGap: theme.spacing(),
        marginBottom: theme.spacing(),
      }}
    >
      {data.map(({ balance, delta }) => {
        const deltaColor = delta.value <= 0 ? theme.error : theme.onTextLight

        return (
          <div key={balance.currency}>
            <CoinsList
              coins={[{ amount: balance.value, unit: balance.currency }]}
            />
            {delta.percentage && delta.value !== 0 && (
              <div style={{ display: 'flex', alignItems: 'center' }}>
                {delta.value < 0 && (
                  <ArrowDown
                    width='24px'
                    height='24px'
                    color={deltaColor}
                    style={{ marginLeft: '-6px' }}
                  />
                )}
                {delta.value > 0 && (
                  <ArrowUp
                    width='24px'
                    height='24px'
                    color={deltaColor}
                    style={{ marginLeft: '-6px' }}
                  />
                )}
                <Text as='span' type='smallMedium' color={deltaColor}>
                  {delta.value.toFixed(2)}%
                </Text>
                <Text
                  as='span'
                  type='smallMedium'
                  color={theme.helpTipText}
                  style={{ display: 'inline-block', marginLeft: '4px' }}
                >
                  {t.mining.statistics.deltas[delta.interval as string]}
                </Text>
              </div>
            )}
            {!delta.percentage && delta.value !== 0 && (
              <div style={{ display: 'flex', alignItems: 'center' }}>
                {delta.value < 0 && (
                  <ArrowDown
                    width='24px'
                    height='24px'
                    color={deltaColor}
                    style={{ marginLeft: '-6px' }}
                  />
                )}
                {delta.value > 0 && (
                  <ArrowUp
                    width='24px'
                    height='24px'
                    color={deltaColor}
                    style={{ marginLeft: '-6px' }}
                  />
                )}
                <CoinsList
                  small
                  color={deltaColor}
                  coins={[{ amount: delta.value, unit: balance.currency }]}
                />
                <Text
                  as='span'
                  type='smallMedium'
                  color={theme.helpTipText}
                  style={{ display: 'inline-block', marginLeft: '4px' }}
                >
                  {t.mining.statistics.deltas[delta.interval as string]}
                </Text>
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}
export default Account
