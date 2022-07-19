import styled from 'styled-components'

export const StyledPagination = styled.div`
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
`

export const PagesContainer = styled.div`
  display: flex;
  justify-content: center;
  align-items: center;

  & > ul {
    list-style: none;
    margin: 0;
    padding-left: 0;
    display: flex;
    cursor: pointer;

    li {
      a {
        display: block;
        box-sizing: border-box;
        border: 1px solid transparent;
        border-radius: ${({ theme }) => theme.borderRadius(0.5)};
        color: ${({ theme }) => theme.primary};
        font-family: 'AvenirMedium';
        text-align: center;

        padding-top: ${({ theme }) => theme.spacingVertical(0.8)};
        padding-bottom: ${({ theme }) => theme.spacingVertical(0.5)};
        padding-left: ${({ theme }) => theme.spacingHorizontal(0.1)};
        padding-right: ${({ theme }) => theme.spacingHorizontal(0.1)};

        min-width: ${({ theme }) => theme.spacingHorizontal(1.6)};

        &:focus {
          outline: none;
        }

        &:hover {
          background: ${({ theme }) => theme.borderColor};
        }
      }

      &.selected a {
        background: ${({ theme }) => theme.accent};
        color: #fff;
        border-color: ${({ theme }) => theme.borderColor};
      }

      &.next {
        margin-left: ${({ theme }) => theme.spacingHorizontal(1)};
      }

      &.previous {
        margin-right: ${({ theme }) => theme.spacingHorizontal(1)};
      }
    }
  }
`

export const PaginationStatsContainer = styled.div`
  margin-top: ${({ theme }) => theme.spacingVertical(4)};
  display: flex;
  align-items: center;
  justify-content: center;
  column-gap: ${({ theme }) => theme.spacingHorizontal(2)};
`

export const SelectContainer = styled.div`
  min-width: ${({ theme }) => theme.spacingHorizontal(4)};
`
