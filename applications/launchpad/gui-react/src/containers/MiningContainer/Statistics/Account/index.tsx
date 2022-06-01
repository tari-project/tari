import { useTheme } from 'styled-components'

import Text from '../../../../components/Text'
import CoinsList from '../../../../components/CoinsList'
import ArrowDown from '../../../../styles/Icons/ArrowBottom2'
import ArrowUp from '../../../../styles/Icons/ArrowTop2'
import t from '../../../../locales'
import { AccountData } from '../types'

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
        const deltaColor =
          delta.percentage <= 0 ? theme.error : theme.onTextLight

        return (
          <div key={balance.currency}>
            <CoinsList
              coins={[{ amount: balance.value, unit: balance.currency }]}
            />
            <div style={{ display: 'flex', alignItems: 'center' }}>
              {delta.percentage <= 0 && (
                <ArrowDown
                  width='24px'
                  height='24px'
                  color={deltaColor}
                  style={{ marginLeft: '-6px' }}
                />
              )}
              {delta.percentage > 0 && (
                <ArrowUp
                  width='24px'
                  height='24px'
                  color={deltaColor}
                  style={{ marginLeft: '-6px' }}
                />
              )}
              <Text as='span' type='smallMedium' color={deltaColor}>
                {delta.percentage}%
              </Text>
              <Text
                as='span'
                type='smallMedium'
                color={theme.secondary}
                style={{ display: 'inline-block', marginLeft: '4px' }}
              >
                {t.mining.statistics.deltas[delta.interval as string]}
              </Text>
            </div>
          </div>
        )
      })}
    </div>
  )
}
export default Account
