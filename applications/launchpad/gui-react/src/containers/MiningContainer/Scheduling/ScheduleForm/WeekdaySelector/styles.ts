import styled from 'styled-components'

export const Weekday = styled.div`
  width: 48px;
  height: 48px;
  background-color: ${({ theme }) => theme.backgroundImage};
  display: flex;
  justify-content: center;
  align-items: center;
  border-radius: 4px;
  cursor: pointer;
`
