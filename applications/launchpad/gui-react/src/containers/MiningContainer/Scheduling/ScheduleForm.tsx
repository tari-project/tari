import { Schedule } from '../../../types/general'

const ScheduleForm = ({
  value,
  onChange,
}: {
  value: Schedule
  onChange: (s: Schedule) => void
}) => {
  return (
    <p>
      editing schedule
      <pre>{JSON.stringify(value, null, 2)}</pre>
    </p>
  )
}

export default ScheduleForm
