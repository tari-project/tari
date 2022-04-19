import { useEffect, useState, Fragment } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import styled from 'styled-components'
import { Listbox } from '@headlessui/react'

import ArrowBottom from '../../styles/Icons/ArrowBottom1'
import { useTheme } from '../../styles'

const people = [
  { id: 1, name: 'Durward Reynolds', unavailable: false },
  { id: 2, name: 'Kenton Towne', unavailable: false },
  { id: 3, name: 'Therese Wunsch', unavailable: false },
  { id: 4, name: 'Benedict Kessler', unavailable: true },
  { id: 5, name: 'Katelyn Rohan', unavailable: false },
]

const WithTheme = (Component) => function TT(props) {
  const theme = useTheme()

  return <Component {...props} theme={theme}/>
}

const SelectorIcon = WithTheme(styled.div`
position: absolute;
top: 0;
right: .5em;
height: 100%;
display: flex;
flex-direction: column;
justify-content: center;
font-size: 1.5em;
color: ${({ theme }) => theme.primary};
`)

const SelectButton = WithTheme(styled(Listbox.Button)`
color: ${({ theme }) => theme.primary};
position: relative;
width: ${({ fullWidth }) => fullWidth ? '100%' : 'auto'};
appearance: none;
background-color: transparent;
padding: 0;
padding: .7em 1.3em;
padding-right: 2em;
margin: 0;
outline: none;
border: none;
border: 1px solid;
border-radius: ${({ theme }) => theme.borderRadius()};
border-color: ${({ theme, open }) => open ? theme.accent : theme.borderColor};
text-align: left;
`)

const Options = WithTheme(styled(Listbox.Options)`
margin: 0;
margin-top: .5em;
padding: 0;
width: ${({ fullWidth }) => fullWidth ? '100%' : 'auto'};
outline: none;
border: 1px solid;
border-radius: ${({ theme }) => theme.borderRadius()};
border-color: ${({ theme, open }) => open ? theme.accent : theme.borderColor};
`)

const Option = WithTheme(styled.li`
list-style-type: none;
margin: .35em 0.65em;
padding: .35em 0.65em;
border-radius: ${({ theme }) => theme.borderRadius(.5)};
background-color: ${({ theme, selected, active }) => selected || active ? theme.selected : 'transparent'};
outline: none;
cursor: default;

&:hover {
  background-color: ${({ theme }) => theme.selected};
}
`)

function MyListbox({ fullWidth }: { fullWidth?: boolean }) {
  const [selectedPerson, setSelectedPerson] = useState(people[0])

  return (
    <Listbox value={selectedPerson} onChange={setSelectedPerson}>
      {({ open }) => <>
        <SelectButton open={open} fullWidth={fullWidth}>
          <span>{selectedPerson.name}</span>
          <SelectorIcon>
            <ArrowBottom />
          </SelectorIcon>
        </SelectButton>
        <Options fullWidth={fullWidth}>
          {people.map((person) => (
            <Listbox.Option key={person.id} value={person} as={Fragment}>
              {({ active, selected }) => (
                <Option selected={selected} active={active}>
                  {person.name}
                </Option>
              )}
            </Listbox.Option>
          ))}
        </Options>
      </>}
    </Listbox>
  )
}

const networks = ['mainnet', 'testnet']

const BaseNodeContainer = () => {
  const [images, setImages] = useState<string[]>([])

  useEffect(() => {
    const getFromBackend = async () => {
      const imagesFromBackend = await invoke<string[]>('image_list')
      setImages(imagesFromBackend)
    }

    getFromBackend()
  }, [])

  return (
    <div>
      <h2>Base Node</h2>
      <p>
        available docker images:
        <br />
        {images.map(img => (
          <em key={img}>
            {img}
            {', '}
          </em>
        ))}
        <br/>
        <MyListbox fullWidth/>
      </p>
    </div>
  )
}

export default BaseNodeContainer
