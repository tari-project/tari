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
  color: ${({inverted, theme}) => inverted ? theme.inverted.primary : theme.primary};
`

export const SelectButton = styled(Listbox.Button)<SelectInternalProps>`
  font-size: 1em;
  color: ${({ theme, inverted }) => inverted ? theme.inverted.primary : theme.primary};
  position: relative;
  width: 100%;
  appearance: none;
  background-color: ${({ theme, inverted }) => inverted ? theme.inverted.controlBackground : theme.controlBackground} ;
  padding: 0;
  padding: ${({ theme }) => `${theme.spacingVertical()} ${theme.spacingHorizontal()}`};
  padding-right: ${({ theme }) => theme.spacingHorizontal()};
  margin: 0;
  outline: none;
  border: none;
  border: 1px solid;
  border-radius: ${({ theme }) => theme.borderRadius()};
  border-color: ${({ theme, inverted, open }) => open ? (inverted ? theme.inverted.accent : theme.accent) : theme.borderColor};
  text-align: left;
`

const FloatingOptions = styled.ul<SelectInternalProps>`
  color: ${({ theme }) => theme.primary};
  position: absolute;
  margin: 0;
  margin-top: ${({ theme }) => theme.spacingVertical()};
  padding: 0;
  width: 100%;
  border: 1px solid;
  border-radius: ${({ theme }) => theme.borderRadius()};
  border-color: ${({ theme, open }) => open ? theme.accent : theme.borderColor};
  background-color: ${({ theme }) => theme.background};
  z-index: 9001;
`

const Options = styled(Listbox.Options)`
  position: relative;
  margin: 0;
  padding: 0;
  width: 100%;
  outline: none;
`

export const OptionsContainer = (props: SelectInternalProps) => <Options {...props}>
  <FloatingOptions {...props}/>
</Options>

export const Option = styled.li<SelectInternalProps & {selected?: boolean; active?: boolean;}>`
  list-style-type: none;
  position: relative;
  padding: ${({ theme }) => `${theme.spacingVertical(0.5)} ${theme.spacingHorizontal(0.5)}`};
  margin: ${({ theme }) => `${theme.spacingVertical(0.5)} ${theme.spacingHorizontal(0.5)}`};
  border-radius: ${({ theme }) => theme.borderRadius(.5)};
  background-color: ${({ theme, selected, active }) => selected || active ? theme.backgroundImage : 'transparent'};
  outline: none;
  cursor: default;

  &:hover {
    background-color: ${({ theme }) => theme.backgroundImage};
  }
`

export const Label = styled(Listbox.Label)`
  font-size: 1em;
  display: inline-block;
  margin-bottom: ${({ theme }) => theme.spacingVertical()};
  color: ${({ theme, darkBackground }) => darkBackground ? theme.background : theme.primary};
`
