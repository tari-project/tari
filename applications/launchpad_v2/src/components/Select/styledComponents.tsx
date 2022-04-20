import styled from 'styled-components'
import { Listbox } from '@headlessui/react'

type SelectInternalProps = {
  darkBackground?: boolean;
  children?: any;
  open?: boolean;
}

export const SelectorIcon = styled.div<SelectInternalProps>`
  position: absolute;
  top: 0;
  right: ${({ theme }) => theme.spacingHorizontal(0.5)};
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  font-size: 1.5em;
  color: ${({darkBackground, theme}) => darkBackground ? theme.background : theme.primary};
`

export const SelectButton = styled(Listbox.Button)<SelectInternalProps>`
  font-size: 1em;
  color: ${({ theme, darkBackground }) => darkBackground ? theme.background : theme.primary};
  position: relative;
  width: 100%;
  appearance: none;
  background-color: ${({ theme, darkBackground }) => darkBackground ? theme.transparentBackground : 'transparent'} ;
  padding: 0;
  padding: ${({ theme }) => `${theme.spacingVertical()} ${theme.spacingHorizontal()}`};
  padding-right: ${({ theme }) => theme.spacingHorizontal()};
  margin: 0;
  outline: none;
  border: none;
  border: 1px solid;
  border-radius: ${({ theme }) => theme.borderRadius()};
  border-color: ${({ theme, darkBackground, open }) => open ? (darkBackground ? theme.background : theme.accent) : theme.borderColor};
  text-align: left;
`

const FloatingOptions = styled.ul<SelectInternalProps>`
  color: ${({ theme }) => theme.primary};
  position: absolute;
  margin: 0;
  padding: 0;
  width: 100%;
  border: 1px solid;
  border-radius: ${({ theme }) => theme.borderRadius()};
  border-color: ${({ theme, open }) => open ? theme.accent : theme.borderColor};
  background-color: ${({ theme }) => theme.background};
`

const Options = styled(Listbox.Options)`
  position: relative;
  margin: 0;
  margin-top: ${({ theme }) => theme.spacingVertical()};
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
  background-color: ${({ theme, selected, active }) => selected || active ? theme.selected : 'transparent'};
  outline: none;
  cursor: default;

  &:hover {
    background-color: ${({ theme }) => theme.selected};
  }
`

export const Label = styled(Listbox.Label)`
  font-size: 1em;
  display: inline-block;
  margin-bottom: ${({ theme }) => theme.spacingVertical()};
  color: ${({ theme, darkBackground }) => darkBackground ? theme.background : theme.primary};
`
