import styled from 'styled-components'

export const NodesContainer = styled.div`
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;

  & > div {
    margin: ${({ theme }) => theme.spacing(0.34)};
  }
  & > div:first-child {
    margin-left: 0;
  }
  & > div:last-child {
    margin-right: 0;
  }
`
