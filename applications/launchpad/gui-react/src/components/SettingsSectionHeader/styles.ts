import styled from 'styled-components'

export const HeaderContainer = styled.div<{
  $noBottomMargin?: boolean
  $noTopMargin?: boolean
}>`
  display: flex;
  align-items: center;
  margin-top: ${({ theme, $noTopMargin }) =>
    $noTopMargin ? theme.spacingVertical(1.5) : theme.spacingVertical(2.7)};
  margin-bottom: ${({ theme, $noBottomMargin }) =>
    $noBottomMargin ? theme.spacingVertical(1.5) : theme.spacingVertical(2.7)};

  & > h2 {
    ${({ theme }) => theme.tariTextGradient};
  }
`

export const HeaderLine = styled.div`
  flex: 1;
  height: 1px;
  background: ${({ theme }) => theme.selectBorderColor};
  margin-left: ${({ theme }) => theme.spacingHorizontal(0.5)};
`
