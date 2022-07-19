import { useTheme } from 'styled-components'

import TrashIcon from '../../../../../styles/Icons/Trash2'
import Text from '../../../../../components/Text'
import Button from '../../../../../components/Button'
import t from '../../../../../locales'

/**
 * @name RemoveSchedule
 * @description button for removing schedule
 *
 * @prop {() => void} remove - callback on click
 */
const RemoveSchedule = ({ remove }: { remove: () => void }) => {
  const theme = useTheme()
  return (
    <div>
      <Button
        variant='text'
        leftIcon={<TrashIcon width='1em' height='1em' />}
        onClick={remove}
        style={{
          paddingLeft: 0,
          display: 'flex',
          color: theme.helpTipText,
        }}
      >
        <Text
          as='label'
          color={theme.helpTipText}
          style={{ cursor: 'pointer', marginTop: theme.spacingVertical(0.3) }}
        >
          {t.mining.scheduling.removeSchedule}
        </Text>
      </Button>
    </div>
  )
}

export default RemoveSchedule
