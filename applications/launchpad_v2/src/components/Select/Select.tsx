import { Fragment } from 'react'
import { Listbox } from '@headlessui/react'

import ArrowBottom from '../../styles/Icons/ArrowBottom1'

import {Label, SelectButton, SelectorIcon, OptionsContainer, Option} from './styledComponents'

type Option = { value: string; label: string; key: string; }
type MyListboxProps = { darkBackground?: boolean; label: string; value: Option; options: Option[]; onChange: (option: Option) => void }

export function Select({ value, options, onChange, darkBackground, label }: MyListboxProps) {
  return (
    <Listbox value={value} onChange={onChange}>
      {({ open }) => <>
        <Label darkBackground={darkBackground}>{label}</Label>
        <SelectButton open={open} darkBackground={darkBackground}>
          <span>{value?.label || ''}</span>
          <SelectorIcon darkBackground={darkBackground}>
            <ArrowBottom />
          </SelectorIcon>
        </SelectButton>
        <OptionsContainer darkBackground={darkBackground}>
          {options.map((option) => (
            <Listbox.Option key={option.key} value={option} as={Fragment}>
              {({ active, selected }) => (
                <Option selected={selected} active={active} darkBackground={darkBackground}>
                  {option.label}
                </Option>
              )}
            </Listbox.Option>
          ))}
        </OptionsContainer>
      </>}
    </Listbox>
  )
}
