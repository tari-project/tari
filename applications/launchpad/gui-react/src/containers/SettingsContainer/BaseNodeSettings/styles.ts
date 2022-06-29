import styled from 'styled-components'

export const SelectRow = styled.div`
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: ${({ theme }) => theme.spacingVertical(2.5)};
`

export const InputRow = styled.div`
  display: flex;
  justify-content: space-between;
  align-items: center;
`

export const ConnectionRow = styled.div`
  margin-top: ${({ theme }) => theme.spacingVertical(1.5)};
  display: flex;
`

export const TextWrapper = styled.div`
  display: flex;
  align-items: center;
  margin-right: ${({ theme }) => theme.spacingHorizontal(0.25)};
`
