import styled from 'styled-components'

import { HelpTipProps } from './types'

export const StyledHelpTipWrapper = styled.div<Pick<HelpTipProps, 'spaced'>>`
  display: flex;
  ${({ theme, spaced }) =>
    spaced ? `margin-top: ${theme.spacingVertical(3)};` : ''}
  ${({ theme, spaced }) =>
    spaced ? `margin-bottom: ${theme.spacingVertical(2.392)};` : ''}
`
