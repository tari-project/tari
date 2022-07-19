import Switch from '../../../../components/Switch'
import { Schedule as ScheduleType } from '../../../../types/general'
import useSingleAndDoubleClick from '../../../../utils/useSingleAndDoubleClick'

import { ScheduleWrapper, ScheduleContainer, ScheduleInfo } from './styles'
import When from './When'
import MiningType from './MiningType'
import Interval from './Interval'

type ScheduleActions = {
  toggle: () => void
  select: () => void
  edit: () => void
}

/**
 * @name Schedule
 * @description Container rendering a single schedule on schedule list
 *
 * @prop {string} id - id of schedule, used only for test-id
 * @prop {boolean} enabled - indicates if the schedule is in enabled state
 * @prop {number[]} [days] - days of the week when schedule is active
 * @prop {Date} [date] - date when schedule is active
 * @prop {Interval} interval - the time window when application should be mining according to this schedule
 * @prop {MiningNodeType[]} type - the types of mining that should be done on this schedule
 * @prop {() => void} toggle - called when user toggles enabled state of the schedule
 * @prop {() => void} select - called when user selects schedule (on single click)
 * @prop {boolean} selected - indicates whether schedule is in selected state
 * @prop {() => void} edit - called when user wants to edit schedule (on double click)
 */
const Schedule = ({
  id,
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
    <ScheduleWrapper selected={selected} data-selected={selected}>
      <ScheduleContainer onClick={clickHandler} data-testid={`schedule-${id}`}>
        <ScheduleInfo>
          <When days={days} date={date} disabled={!enabled} />
          <Interval {...interval} disabled={!enabled} />
          <MiningType type={type} disabled={!enabled} />
        </ScheduleInfo>
        <Switch value={enabled} onClick={toggle} inverted={true} />
      </ScheduleContainer>
    </ScheduleWrapper>
  )
}

export default Schedule
