import { TextareaHTMLAttributes } from 'react'
import styled from 'styled-components'

export const InputContainer = styled.div`
  padding: 2px 6px;
  border: 1px solid;
  border-color: ${({ theme }) => theme.borderColor};
  border-radius: 8px;
  width: 100%;
  display: flex;
  box-sizing: border-box;
`

export const StyledTextarea = styled.textarea<
  TextareaHTMLAttributes<HTMLTextAreaElement>
>`
  width: 100%;
  padding: 10px;
  box-sizing: border-box;
  font-family: 'AvenirMedium';
  font-size: 14px;
  line-height: inherit;
  color: ${({ theme, disabled }) => {
    if (disabled) {
      return theme.placeholderText
    } else {
      return theme.primary
    }
  }};
  background-color: ${({ theme, disabled }) =>
    disabled ? theme.backgroundImage : theme.background};
  border: none;
  border-radius: 8px;
  resize: vertical;

  ::placeholder {
    color: ${({ theme }) => theme.placeholderText};
  }

  &:focus {
    outline: none;
    color: ${({ theme }) => {
      return theme.primary
    }};
  }

  ::-webkit-scrollbar {
    width: 4px;
  }

  /* Track */
  ::-webkit-scrollbar-track {
    background: transparent;
  }

  /* Handle */
  ::-webkit-scrollbar-thumb {
    background: ${({ theme }) => theme.borderColor};
    border-radius: 3px;
  }

  /* Handle on hover */
  ::-webkit-scrollbar-thumb:hover {
    background: #555;
  }
`

export const ErrorContainer = styled.div`
  display: flex;
  min-height: 25px;
  padding-top: ${({ theme }) => theme.spacing(0.075)};
  padding-bottom: ${({ theme }) => theme.spacing(0.125)};
  padding-left: ${({ theme }) => theme.spacing(0.25)};
  padding-right: ${({ theme }) => theme.spacing(0.25)};
`
