import styled from 'styled-components'

export const TabsContainer = styled.div`
  display: flex;
  align-items: flex-start;
  flex-direction: column;
`

export const Tab = styled.div<{ selected?: boolean }>`
  display: flex;
  padding: 12px;
  border-bottom: ${({ selected }) =>
    selected ? '4px solid #9330FF' : '4px solid #fff'};
`

export const TabOptions = styled.div`
  display: flex;
  align-items: center;
`

export const TabsBorder = styled.div`
  height: 4px;
  background: red;
  width: 100%;
`

export const TabBorderSelection = styled.div`
  height: 100%;
  background: blue;
  width: 50px;
`
