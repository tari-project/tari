import styled from 'styled-components'

export const StyledBox = styled.div`
  background: ${({ theme }) => theme.backgroundImage};
  border: 1px solid ${({ theme }) => theme.borderColor};
  border-radius: ${({ theme }) => theme.tightBorderRadius()};
  color: ${({ theme }) => theme.secondary};
  padding: ${({ theme }) => theme.spacingVertical()}
    ${({ theme }) => theme.spacingHorizontal()};
  margin: ${({ theme }) => theme.spacingVertical(0.6)} 0;
  box-sizing: border-box;
  display: flex;
  justify-content: space-between;
  column-gap: 0.25em;
`

export const FeedbackContainer = styled.div`
  position: absolute;
  left: 50%;
  bottom: 120%;
  transform: translateX(-50%);
`

export const ValueContainer = styled.div`
  overflow-x: hidden;
  text-overflow: ellipsis;
  word-break: keep-all;
  -webkit-user-select: none;
  cursor: default;
  font-family: 'AvenirMedium';
`
