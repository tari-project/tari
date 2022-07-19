import { Time } from '../../../../types/general'

export const utcTimeToString = (t?: Time) => {
  const timezoneHourOffset = new Date().getTimezoneOffset() / 60
  const hours = t ? t?.hours - timezoneHourOffset : 0

  return `${hours.toString().padStart(2, '0') || '00'}:${
    t?.minutes.toString().padStart(2, '0') || '00'
  }`
}
export const stringToUTCTime = (s: string): Time => {
  const timezoneHourOffset = new Date().getTimezoneOffset() / 60

  return {
    hours: Number(s.substring(0, 2)) + timezoneHourOffset,
    minutes: Number(s.substring(3, 5)),
  }
}
