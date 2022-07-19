import { useTheme } from 'styled-components'

import Text from '../../../../../components/Text'
import * as Format from '../../../../../utils/Format'
import t from '../../../../../locales'

import { TooltipWrapper, SeriesColorIndicator } from './styles'

export type TooltipProps = {
  display?: boolean
  left?: number
  top?: number
  x?: Date
  values?: {
    service: string
    unit: string
    value: number | null
    color: string
  }[]
}

const Tooltip = ({ display, left, top, values, x }: TooltipProps) => {
  const theme = useTheme()

  return (
    <TooltipWrapper
      style={{
        display: display ? 'block' : 'none',
        left,
        top,
      }}
    >
      {Boolean(values) && (
        <ul>
          {(values || [])
            .filter(v => Boolean(v.value))
            .map(v => (
              <li key={`${v.service}${v.value}`}>
                <SeriesColorIndicator color={v.color} />
                <Text type='smallMedium' color={theme.inverted.lightTagText}>
                  {t.common.containers[v.service]}{' '}
                  <span style={{ color: theme.inverted.primary }}>
                    {v.value}
                    {v.unit}
                  </span>
                </Text>
              </li>
            ))}
        </ul>
      )}
      {Boolean(x) && (
        <Text type='smallMedium' color={theme.inverted.lightTagText}>
          {/* eslint-disable-next-line @typescript-eslint/no-non-null-assertion */}
          {Format.dateTime(x!)}
        </Text>
      )}
    </TooltipWrapper>
  )
}

export default Tooltip
