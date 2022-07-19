import { CSSProperties } from 'react'
import styled from 'styled-components'
import { Listbox } from '@headlessui/react'

import { SelectInternalProps } from './types'

export const SelectorIcon = styled.div<SelectInternalProps>`
  position: absolute;
  top: 0;
  right: ${({ theme }) => theme.spacingHorizontal(0.5)};
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  font-size: 1.5em;
  color: ${({ inverted, theme }) =>
    inverted ? theme.inverted.primary : theme.primary};
`

export const SelectButton = styled(Listbox.Button)<
  SelectInternalProps & {
    style?: { borderColor?: (open?: boolean) => string } & Omit<
      CSSProperties,
      'borderColor'
    >
  }
>`
  display: flex;
  align-items: center;
  column-gap: 0.3em;
  cursor: ${({ disabled }) => (disabled ? 'default' : 'pointer')};
  font-size: 1em;
  color: ${({ theme, inverted }) =>
    inverted ? theme.inverted.primary : theme.secondary};
  position: relative;
  appearance: none;
  background-color: ${({ theme, inverted }) =>
    inverted ? theme.inverted.controlBackground : theme.controlBackground};
  padding: ${({ theme }) =>
    `${theme.spacingVertical(0.78)} ${theme.spacingHorizontal(0.67)}`};
  padding-right: ${({ theme }) => theme.spacingHorizontal(1.5)};
  width: ${({ fullWidth }) => (fullWidth ? '100%' : '')};
  margin: 0;
  outline: none;
  border: none;
  border: 1px solid;
  border-radius: ${({ theme }) => theme.tightBorderRadius()};
  border-color: ${({ style, theme, inverted, open }) => {
    if (style?.borderColor) {
      return style.borderColor(open)
    }

    return open
      ? inverted
        ? theme.inverted.accent
        : theme.accent
      : theme.selectBorderColor
  }};
  text-align: left;
`

const FloatingOptions = styled.ul<SelectInternalProps>`
  color: ${({ theme }) => theme.secondary};
  position: absolute;
  margin: 0;
  margin-top: ${({ theme }) => theme.spacingVertical(0.5)};
  padding: 0;
  padding-top: ${({ theme }) => theme.spacingVertical(0.4)};
  padding-bottom: ${({ theme }) => theme.spacingVertical(0.4)};
  width: ${({ fullWidth }) => (fullWidth ? '100%' : 'auto')};
  border: 1px solid;
  border-radius: ${({ theme }) => theme.tightBorderRadius()};
  border-color: ${({ theme, open }) =>
    open ? theme.accent : theme.selectBorderColor};
  background-color: ${({ theme }) => theme.nodeBackground};
  z-index: 9001;
`

const Options = styled(Listbox.Options)`
  position: relative;
  margin: 0;
  padding: 0;
  width: 100%;
  outline: none;
`

export const OptionsContainer = (props: SelectInternalProps) => (
  <Options {...props}>
    <FloatingOptions {...props} />
  </Options>
)

export const Option = styled.li<
  SelectInternalProps & { selected?: boolean; active?: boolean }
>`
  list-style-type: none;
  position: relative;
  padding: ${({ theme }) =>
    `${theme.spacingVertical(0.4)} ${theme.spacingHorizontal(0.4)}`};
  margin: ${({ theme }) =>
    `${theme.spacingVertical(0.4)} ${theme.spacingHorizontal(0.4)}`};
  border-radius: ${({ theme }) => theme.borderRadius(0.5)};

  outline: none;
  cursor: pointer;

  &:hover {
    background-color: ${({ theme }) => theme.selectOptionHover};
  }
`

export const Label = styled(Listbox.Label)<
  SelectInternalProps & { style?: { color?: string } }
>`
  font-size: 1em;
  display: inline-block;
  margin-bottom: ${({ theme }) => theme.spacingVertical()};
  color: ${({ style }) => style?.color};
  font-family: 'AvenirMedium';
`
