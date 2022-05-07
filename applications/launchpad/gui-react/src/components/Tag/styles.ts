import styled, { CSSProperties } from 'styled-components'

export const TagContainer = styled.div`
  display: flex;
  flex-direction: row;
  justify-content: center;
  align-items: center;
  border-radius: 64px;
  height: 26px;
  border: 0;
  width: fit-content;
  padding-left: 12px;
  padding-right: 12px;
`

export const IconWrapper = styled.div<{
  type?: string
  textStyle?: CSSProperties
}>`
  display: flex;
  align-items: center;
  color: white;
  height: 100%;
  margin-right: 7.5px;
  color: ${({ theme, type, textStyle }) =>
    type === 'expert' ? theme.accent : textStyle?.color};
`
