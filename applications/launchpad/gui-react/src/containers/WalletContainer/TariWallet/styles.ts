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
