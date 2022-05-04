import styled from 'styled-components'

export const StyledBox = styled.div`
  color: ${({ theme }) => theme.primary};
  background: ${({ theme }) => theme.background};
  padding: ${({ theme }) => theme.spacing()};
  margin: ${({ theme }) => theme.spacing()} 0;
  border-radius: ${({ theme }) => theme.borderRadius()};
  border: 1px solid ${({ theme }) => theme.borderColor};
  min-width: 416px;
  width: 416px;
  box-sizing: border-box;
`
