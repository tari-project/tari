import styled from 'styled-components'

export const HeaderContainer = styled.div`
  display: flex;
  align-items: center;
  margin: ${({ theme }) => theme.spacingVertical(2.7)} 0;

  & > h2 {
    ${({ theme }) => theme.tariTextGradient};
  }
`

export const HeaderLine = styled.div`
  flex: 1;
  height: 1px;
  background: ${({ theme }) => theme.borderColor};
  margin-left: ${({ theme }) => theme.spacingHorizontal(0.5)};
`
