import { useAppSelector } from '../../store/hooks'

import Tag from '../../components/Tag'
import { selectRunning, selectHealthy } from '../../store/baseNode/selectors'
import t from '../../locales'

const StateTag = () => {
  const running = useAppSelector(selectRunning)
  const healthy = useAppSelector(selectHealthy)

  if (!running) {
    return null
  }

  if (!healthy) {
    return (
      <Tag variant='small' type='warning'>
        {t.common.adjectives.unhealthy}
      </Tag>
    )
  }

  return (
    <Tag variant='small' type='running'>
      {t.common.adjectives.running}
    </Tag>
  )
}

export default StateTag
