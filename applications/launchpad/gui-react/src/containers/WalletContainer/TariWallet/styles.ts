import styled from 'styled-components'

export const SemiTransparent = styled.span`
  opacity: 0.7;
`

export const TariIdContainer = styled.div`
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  column-gap: ${({ theme }) => theme.spacing(0.5)};
`

export const TariIdBox = styled.div`
  font-size: 14px;
  flex-grow: 1;
  border: 1px solid ${({ theme }) => theme.borderColor};
  border-radius: ${({ theme }) => theme.tightBorderRadius()};
  padding: ${({ theme }) => theme.spacingVertical()}
    ${({ theme }) => theme.spacingHorizontal(0.75)};
  color: ${({ theme }) => theme.borderColor};
  background-color: ${({ theme }) => theme.resetBackground};
`
