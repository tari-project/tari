import styled from 'styled-components'

export const IconsWrapper = styled.div`
  display: inline-block;
  vertical-align: baseline;
`

export const KeyTile = styled.span`
  display: inline-block;
  vertical-align: middle;
  text-align: center;
  font-size: 10px;
  line-height: 10px;
  padding: 2px;
  background: transparent;
  border: 1px solid ${({ theme }) => theme.borderColorLight};
  border-radius: 4px;
  min-width: 16px;
  height: 16px;
  box-sizing: border-box;
  margin-left: 1px;
  margin-right: 1px;
  margin-top: -4%;
`

export const LetterKey = styled.span`
  text-align: center;
  font-size: 12px;
  line-height: 12px;
  font-weight: 500;
`
