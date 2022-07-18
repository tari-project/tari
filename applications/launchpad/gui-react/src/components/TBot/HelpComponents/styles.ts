import styled from 'styled-components'

export const StyledTextContainer = styled.div`
  display: flex;
  margin-bottom: ${({ theme }) => theme.spacingVertical(2)};
`

export const ListItem = styled.li`
  font-family: 'AvenirMedium';
  margin-bottom: ${({ theme }) => theme.spacingVertical(0.5)};
`

export const ListGroup = styled.ul`
  margin-top: ${({ theme }) => theme.spacingVertical(0.5)};
  margin-left: ${({ theme }) => theme.spacingHorizontal(-0.5)};
`
