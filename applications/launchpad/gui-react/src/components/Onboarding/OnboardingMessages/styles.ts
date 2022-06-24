import styled from 'styled-components'

export const CtaButtonContainer = styled.div<{ $noMargin?: boolean }>`
  display: inline-flex;
  ${({ theme, $noMargin }) =>
    !$noMargin ? `margin-top: ${theme.spacingVertical(1)};` : ''}
`
