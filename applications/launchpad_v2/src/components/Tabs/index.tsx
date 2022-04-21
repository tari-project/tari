import { TabsContainer, Tab, TabOptions } from './styles'
import { TabsProps } from './types'

const Tabs = ({ tabs, selected, onSelect }: TabsProps) => {
  return (
    <TabsContainer>
      <TabOptions>
        {tabs.map((tab, index) => (
          <Tab
            key={`tab-${index}`}
            selected={selected === tab.id}
            onClick={() => onSelect(tab.id)}
          >
            {tab.content}
          </Tab>
        ))}
      </TabOptions>
    </TabsContainer>
  )
}

export default Tabs
