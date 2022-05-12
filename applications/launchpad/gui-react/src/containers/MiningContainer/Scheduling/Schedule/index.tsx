import Switch from '../../../../components/Switch'

import { Schedule as ScheduleType } from '../types'

import { ScheduleContainer, ScheduleInfo } from './styles'
import When from './When'
import MiningType from './MiningType'
import Interval from './Interval'

type ScheduleActions = {
  toggle: () => void
  select: () => void
  edit: () => void
  remove: () => void
}

const Schedule = ({
  enabled,
  days,
  date,
  interval,
  type,
  toggle,
}: ScheduleType & { selected: boolean } & ScheduleActions) => {
  return (
    <ScheduleContainer>
      <ScheduleInfo>
        <When days={days} date={date} disabled={!enabled} />
        <Interval {...interval} disabled={!enabled} />
        <MiningType type={type} disabled={!enabled} />
      </ScheduleInfo>
      <Switch value={enabled} onClick={toggle} />
    </ScheduleContainer>
  )
}

export default Schedule
