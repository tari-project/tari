import styled from 'styled-components'

export const TagContainer = styled.div<{ variant?: string }>`
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

export const IconWrapper = styled.div`
  display: flex;
  align-items: center;
  color: white;
  height: 100%;
  margin-right: 7.5px;
`
