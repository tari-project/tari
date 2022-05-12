import Switch from '../../../../components/Switch'
import { Schedule as ScheduleType } from '../../../../types/general'

import { ScheduleContainer, ScheduleInfo } from './styles'
import When from './When'
import MiningType from './MiningType'
import Interval from './Interval'
import useSingleAndDoubleClick from '../../../../utils/useSingleAndDoubleClick'

type ScheduleActions = {
  toggle: () => void
  select: () => void
  edit: () => void
}

const Schedule = ({
  enabled,
  days,
  date,
  interval,
  type,
  toggle,
  selected,
  select,
  edit,
}: ScheduleType & { selected: boolean } & ScheduleActions) => {
  const clickHandler = useSingleAndDoubleClick({
    single: select,
    double: edit,
  })

  return (
    <ScheduleContainer
      onClick={clickHandler}
      selected={selected}
      data-selected={selected}
    >
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
