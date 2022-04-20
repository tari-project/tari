import styled from 'styled-components'
import { Listbox } from '@headlessui/react'

import { WithTheme } from '../../styles'

export const SelectorIcon = WithTheme(styled.div`
  position: absolute;
  top: 0;
  right: ${({ theme }) => theme.spacingHorizontal(0.5)};
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  font-size: 1.5em;
  color: ${({onDark, theme}: any) => onDark ? theme.background : theme.primary};
`)

export const SelectButton = WithTheme(styled(Listbox.Button)`
  font-size: 1em;
  color: ${({ theme, onDark }: any) => onDark ? theme.background : theme.primary};
  position: relative;
  width: ${({ fullWidth }: any) => fullWidth ? '100%' : 'auto'};
  appearance: none;
  background-color: ${({ onDark }: any) => onDark ? 'rgba(255,255,255,.2)' : 'transparent'} ;
  padding: 0;
  padding: ${({ theme }) => `${theme.spacingVertical()} ${theme.spacingHorizontal()}`};
  padding-right: ${({ theme }: any) => theme.spacingHorizontal()};
  margin: 0;
  outline: none;
  border: none;
  border: 1px solid;
  border-radius: ${({ theme }) => theme.borderRadius()};
  border-color: ${({ theme, onDark, open }: any) => open ? (onDark ? theme.background : theme.accent) : theme.borderColor};
  text-align: left;
`)

const FloatingOptions = WithTheme(styled.ul`
  color: ${({ theme, onDark }: any) => onDark ? theme.background : theme.primary};
  position: absolute;
  margin: 0;
  padding: 0;
  width: 100%;
  border: 1px solid;
  border-radius: ${({ theme }) => theme.borderRadius()};
  border-color: ${({ theme, open }: any) => open ? theme.accent : theme.borderColor};
  background-color: ${({ theme }) => theme.background};
`)

const Options = WithTheme(styled(Listbox.Options)`
  position: relative;
  margin: 0;
  margin-top: ${({ theme }) => theme.spacingVertical()};
  padding: 0;
  width: ${({ fullWidth }: any) => fullWidth ? '100%' : 'auto'};
  outline: none;
`)

export const OptionsContainer = (props: any) => <Options {...props}>
  <FloatingOptions {...props}/>
</Options>

export const Option = WithTheme(styled.li`
  list-style-type: none;
  position: relative;
  padding: ${({ theme }) => `${theme.spacingVertical(0.5)} ${theme.spacingHorizontal(0.5)}`};
  margin: ${({ theme }) => `${theme.spacingVertical(0.5)} ${theme.spacingHorizontal(0.5)}`};
  border-radius: ${({ theme }) => theme.borderRadius(.5)};
  background-color: ${({ theme, selected, active }: any) => selected || active ? theme.selected : 'transparent'};
  outline: none;
  cursor: default;

  &:hover {
    background-color: ${({ theme }) => theme.selected};
  }
`)

export const Label = WithTheme(styled(Listbox.Label)`
  font-size: 1em;
  display: inline-block;
  margin-bottom: ${({ theme }) => theme.spacingVertical()};
  color: ${({ theme, onDark }) => onDark ? theme.background : theme.primary};
`)
