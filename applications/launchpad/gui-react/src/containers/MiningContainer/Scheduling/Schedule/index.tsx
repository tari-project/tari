import Switch from '../../../../components/Switch'

import { Schedule as ScheduleType } from '../types'

import { ScheduleContainer, ScheduleInfo } from './styles'
import When from './When'
import MiningType from './MiningType'
import Interval from './Interval'

const Schedule = ({ enabled, days, date, interval, type }: ScheduleType) => {
  return (
    <ScheduleContainer>
      <ScheduleInfo>
        <When days={days} date={date} disabled={!enabled} />
        <Interval {...interval} disabled={!enabled} />
        <MiningType type={type} disabled={!enabled} />
      </ScheduleInfo>
      <Switch value={enabled} onClick={() => console.log(`to ${enabled}`)} />
    </ScheduleContainer>
  )
}

export default Schedule
