import styled from 'styled-components'
import { CalloutType } from './types'

export const StyledCallout = styled.div<{
  $type: CalloutType
  $inverted?: boolean
}>`
  padding: ${({ theme }) =>
    `${theme.spacingVertical(0.62)} ${theme.spacingHorizontal(0.5)}`};
  background: ${({ theme, $inverted }) => {
    return $inverted
      ? theme.inverted.backgroundSecondary
      : theme.calloutBackground
  }}};
  color: ${({ theme, $inverted }) => {
    return $inverted ? theme.inverted.warningText : theme.warningText
  }};
  border-radius: ${({ theme }) => theme.borderRadius(0.5)};
  font-size: 0.9em;
  line-height: 150%;
`

export const CalloutIcon = styled.span`
  display: inline-block;
  font-size: 12px;
  margin-right: ${({ theme }) => theme.spacingHorizontal(0.15)};
`
