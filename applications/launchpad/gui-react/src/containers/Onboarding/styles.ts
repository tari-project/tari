import styled from 'styled-components'

export const StyledOnboardingContainer = styled.div`
  display: flex;
  flex: 1;
  flex-direction: column;
  width: 100%;
  align-items: center;
  justify-content: flex-end;
  background-color: ${({ theme }) => theme.backgroundSecondary};
  padding-bottom: 190px;
`
