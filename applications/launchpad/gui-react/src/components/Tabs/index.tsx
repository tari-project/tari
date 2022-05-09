import { useEffect, useRef, useState } from 'react'
import { useSpring } from 'react-spring'
import { useTheme } from 'styled-components'

import Text from '../Text'

import {
  TabsContainer,
  Tab,
  TabOptions,
  TabContent,
  TabSelectedBorder,
  FontWeightCompensation,
} from './styles'

import { TabsProps } from './types'

/**
 * Tabs component renders the set of tab header tiles.
 *
 * @param {TabsProps} props - props of the Tabs component
 *
 * @typedef TabsProps
 * @param {TabProp[]} tabs - the list of tabs.
 * @param {string} selected - the id of the selected tab. It has to match the `id` prop of the tab.
 * @param {(val: string) => void} onSelect - on tab click.
 *
 * @typedef TabProp
 * @param {string} id - unique identifier of the tab
 * @param {ReactNode} content - the tab header content
 */
const Tabs = ({ tabs, selected, onSelect, inverted }: TabsProps) => {
  const tabsRefs = useRef<(HTMLButtonElement | null)[]>([])

  // The animation of the bottom 'border' that indicates the selected tab,
  // is based on sizes of the rendered tabs. It means, that the componenets
  // have to be rendered first, then the parent component can read widths,
  // and finally the size and shift can ba calculated.
  // Also, the Tabs component needs to re-render tabs twice on the initial mount,
  // because the selected tab uses bold font, which changes tabs widths.
  const [initialized, setInitialzed] = useState(0)
  const theme = useTheme()

  useEffect(() => {
    tabsRefs.current = tabsRefs.current.slice(0, tabs.length)
    setInitialzed(1)
  }, [tabs])

  useEffect(() => {
    if (initialized < 2) {
      setInitialzed(initialized + 1)
    }
  }, [initialized])

  const selectedIndex = tabs.findIndex(t => t.id === selected)
  let width = 0
  let left = 0
  let totalWidth = 0
  const tabMargin = theme.tabsMarginRight

  if (selectedIndex > -1) {
    if (
      tabsRefs &&
      tabsRefs.current &&
      tabsRefs.current.length > selectedIndex
    ) {
      tabsRefs.current.forEach((el, index) => {
        if (el) {
          if (index < selectedIndex) {
            left = left + el.offsetWidth + tabMargin
          } else if (index === selectedIndex) {
            width = el.offsetWidth
          }
          totalWidth = totalWidth + el.offsetWidth
        }
      })
    }
  }

  const activeBorder = useSpring({
    to: { left: left, width: width },
    config: { duration: 100 },
  })

  return (
    <TabsContainer>
      <TabOptions>
        {tabs.map((tab, index) => (
          <Tab
            key={`tab-${index}`}
            ref={el => (tabsRefs.current[index] = el)}
            onClick={() => onSelect(tab.id)}
            selected={selected}
            tab={tab}
            $inverted={inverted}
          >
            <FontWeightCompensation>
              <Text
                as={'span'}
                type='defaultHeavy'
                style={{ whiteSpace: 'nowrap', width: '100%' }}
              >
                {tab.content}
              </Text>
            </FontWeightCompensation>
            <TabContent>
              <Text
                as={'span'}
                type={selected === tab.id ? 'defaultHeavy' : 'defaultMedium'}
                style={{ whiteSpace: 'nowrap', width: '100%' }}
                color={inverted ? theme.inverted.primary : undefined}
              >
                {tab.content}
              </Text>
            </TabContent>
          </Tab>
        ))}
      </TabOptions>
      <TabSelectedBorder
        $inverted={inverted}
        style={{
          ...activeBorder,
        }}
      />
    </TabsContainer>
  )
}

export default Tabs
