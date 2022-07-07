import styled from 'styled-components'

import { HelpTipProps } from './types'

export const StyledHelpTipWrapper = styled.div<Pick<HelpTipProps, 'header'>>`
  display: flex;
  ${({ theme, header }) =>
    header ? `margin-top: ${theme.spacingVertical(3)};` : ''}
  color: ${({ theme }) => theme.helpTipText}
`
