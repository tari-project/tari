import styled from 'styled-components'

export const StyledMoneroURL = styled.div`
  border: 1px solid ${({ theme }) => theme.borderColor};
  border-radius: ${({ theme }) => theme.borderRadius()};
  padding: ${({ theme }) => theme.spacingHorizontal(0.67)};
  margin-bottom: ${({ theme }) => theme.spacingVertical(1)};
  padding-bottom: ${({ theme }) => theme.spacingHorizontal(1.8)};

  &:hover {
    background: ${({ theme }) => theme.backgroundSecondary};
  }

  & > input {
    width: 100%;
  }

  &:hover .header-buttons {
    display: flex !important;
  }
`

export const HeaderRow = styled.div`
  display: flex;
  align-items: center;
  justify-content: space-between;

  & > .header-buttons {
    margin-bottom: ${({ theme }) => theme.spacingVertical(0.67)};
    display: none;
  }
`
