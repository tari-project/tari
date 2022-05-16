import { Time } from '../../../../types/general'

export const timeToString = (t?: Time) =>
  `${t?.hours.toString().padStart(2, '0') || '00'}:${
    t?.minutes.toString().padStart(2, '0') || '00'
  }`
export const stringToTime = (s: string): Time => ({
  hours: Number(s.substring(0, 2)),
  minutes: Number(s.substring(3, 5)),
})
