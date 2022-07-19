import { useTheme } from 'styled-components'

import { MiningNodeType } from '../../../../types/general'
import Text from '../../../../components/Text'
import t from '../../../../locales'

/**
 * @name MiningType
 * @description renders comma separate list of translated strings of mining type
 *
 * @prop {MiningNodeType[]} type - list of types of mining
 * @prop {boolean} disabled - indicates whether to render in disabled UI state
 */
const MiningType = ({
  type,
  disabled,
}: {
  type: MiningNodeType[]
  disabled: boolean
}) => {
  const miningTypeString = type
    .map((miningType: MiningNodeType) => t.common.miningType[miningType])
    .join(', ')
  const theme = useTheme()

  const color = disabled ? theme.inputPlaceholder : theme.primary

  return (
    <Text type='smallMedium' color={color}>
      {miningTypeString}
    </Text>
  )
}

export default MiningType
