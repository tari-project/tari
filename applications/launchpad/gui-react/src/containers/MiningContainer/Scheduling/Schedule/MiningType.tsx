import { useTheme } from 'styled-components'

import { MiningNodeType } from '../../../../types/general'
import Text from '../../../../components/Text'
import t from '../../../../locales'

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

  const color = disabled ? theme.placeholderText : undefined

  return (
    <Text type='smallMedium' color={color}>
      {miningTypeString}
    </Text>
  )
}

export default MiningType
