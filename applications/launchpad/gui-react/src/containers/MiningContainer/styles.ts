import styled from 'styled-components'

export const NodesContainer = styled.div`
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  margin-left: -${({ theme }) => theme.spacing(0.68)};
  margin-right: -${({ theme }) => theme.spacing(0.68)};

  & > div {
    margin: ${({ theme }) => theme.spacing(0.68)};
  }
`
