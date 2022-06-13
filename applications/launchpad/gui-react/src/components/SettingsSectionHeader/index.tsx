import Text from '../Text'
import { HeaderContainer, HeaderLine } from './styles'
import { SettingsSectionHeaderProps } from './types'

/**
 * Settings header containing a text and line.
 */
const SettingsSectionHeader = ({ children }: SettingsSectionHeaderProps) => {
  return (
    <HeaderContainer data-testid='settings-section-header-cmp'>
      {children && (
        <Text type='microHeavy' as='h2'>
          {children}
        </Text>
      )}
      <HeaderLine />
    </HeaderContainer>
  )
}

export default SettingsSectionHeader
