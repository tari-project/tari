import { useTheme } from 'styled-components'

import { MiningNodeType } from '../../../../../types/general'
import Checkbox from '../../../../../components/Checkbox'
import t from '../../../../../locales'

/**
 * @name MiningTypeSelector
 * @description set of checkboxes for mining types
 *
 * @prop {MiningNodeType[]} [value] - initial values of selected MiningNodeType
 * @prop {(v: MiningNodeType[]) => void} onChange - callback called when value of any checkbox changes
 * @prop {MiningNodeType[]} miningTypesActive - which mining types can be chosen
 */
const MiningTypeSelector = ({
  value,
  onChange,
  miningTypesActive,
}: {
  value: MiningNodeType[]
  onChange: (v: MiningNodeType[]) => void
  miningTypesActive: MiningNodeType[]
}) => {
  const theme = useTheme()

  const toggle = (v: MiningNodeType) => {
    if (value.includes(v)) {
      onChange(value.filter(type => type !== v))

      return
    }

    onChange([...value, v])
  }

  return (
    <div>
      <Checkbox
        checked={value.includes('tari')}
        onChange={() => toggle('tari')}
        style={{ marginBottom: theme.spacing(0.75) }}
        disabled={!miningTypesActive.includes('tari')}
      >
        {t.common.miningType['tari']}
      </Checkbox>
      <Checkbox
        checked={value.includes('merged')}
        onChange={() => toggle('merged')}
        disabled={!miningTypesActive.includes('merged')}
      >
        {t.common.miningType['merged']}
      </Checkbox>
    </div>
  )
}
MiningTypeSelector.defaultProps = {
  value: [],
}

export default MiningTypeSelector
