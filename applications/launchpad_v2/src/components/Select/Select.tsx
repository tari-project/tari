import { Fragment } from 'react'
import { Listbox } from '@headlessui/react'

import ArrowBottom from '../../styles/Icons/ArrowBottom1'

import {Label, SelectButton, SelectorIcon, OptionsContainer, Option} from './styledComponents'

type Option = { value: string; label: string; key: string; }
type MyListboxProps = { inverted?: boolean; label: string; value: Option; options: Option[]; onChange: (option: Option) => void }

export function Select({ value, options, onChange, inverted, label }: MyListboxProps) {
  return (
    <Listbox value={value} onChange={onChange}>
      {({ open }) => <>
        <Label darkBackground={inverted}>{label}</Label>
        <SelectButton open={open} inverted={inverted}>
          <span>{value?.label || ''}</span>
          <SelectorIcon inverted={inverted}>
            <ArrowBottom />
          </SelectorIcon>
        </SelectButton>
        <OptionsContainer inverted={inverted}>
          {options.map((option) => (
            <Listbox.Option key={option.key} value={option} as={Fragment}>
              {({ active, selected }) => (
                <Option selected={selected} active={active} inverted={inverted}>
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
