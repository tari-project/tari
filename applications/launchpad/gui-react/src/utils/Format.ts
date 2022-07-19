export const dateTime = (d: Date): string =>
  `${d.toLocaleDateString()} ${d.toLocaleTimeString()}`

export const localHour = (d: Date): string =>
  d.toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  })

export const utcHour = ({
  hours,
  minutes,
}: {
  hours: number
  minutes: number
}) => {
  const date = new Date()
  date.setUTCHours(hours)
  date.setUTCMinutes(minutes)

  return date.toLocaleTimeString([], {
    hour: 'numeric',
    minute: '2-digit',
    hour12: true,
  })
}

export const day = (date: Date) =>
  date.toLocaleDateString([], {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  })

export const month = (date: Date) =>
  date.toLocaleDateString([], { year: 'numeric', month: 'long' })

export const shortMonth = (date: Date) =>
  date.toLocaleDateString([], { year: 'numeric', month: 'short' })

/**
 * Convert milliseconds to 0:00:00 {hours:minutes:seconds} format.
 * @param {number} time - milliseconds
 *
 * @example
 * humanizeTime(10000000) // '02:46:40'
 */
export const humanizeTime = (time: number): string => {
  const hours = Math.trunc(time / 3600000).toString()
  const minutes = (Math.trunc(time / 60000) % 60).toString()
  const seconds = (Math.trunc(time / 1000) % 60).toString()

  return `${hours.padStart(1, '0')}:${minutes.padStart(
    2,
    '0',
  )}:${seconds.padStart(2, '0')}`
}

/**
 * Convert milliseconds to the "estimated time".
 * It will display only significant time components: hours, minutes, seconds,
 * depending on the time value.
 * For instance, it will display seconds if remaining less than 3 minutes.
 * @param {time} time - the time in milliseconds
 *
 * @example
 * humanizeEstimatedTime(4500)
 */
export const humanizeEstimatedTime = (time: number): string => {
  const h = Math.floor(time / 3600)
  const m = Math.floor((time % 3600) / 60)
  const s = Math.floor((time % 3600) % 60)
  let result = ''

  if (h > 0) {
    result += `${h}h `
  }
  if (m > 0) {
    result += `${m} min `
  }

  if (m < 3) {
    result += `${s} s`
  }

  return result.trim()
}

/**
 * Convert Tauri to micro Tauri (uT) value
 * @param {number} amount amount in Tari
 * @returns {number}
 */
export const toMicroT = (amount: number): number => {
  return amount * 1000000
}

/**
 * Convert micro Tauri to Tauri value
 * @param {number} amount amount in micro Tari (uT)
 * @returns {number}
 */
export const toT = (amount: number): number => {
  return amount / 1000000
}

/**
 * Format the coin amount
 * @param {number} amount
 * @returns {string}
 */
export const formatAmount = (amount: string | number): string => {
  if (Number(amount) === 0) {
    return '00,000'
  } else {
    try {
      return Number(amount).toLocaleString([], { maximumFractionDigits: 2 })
    } catch (err) {
      return '-'
    }
  }
}

/**
 * Convert array of U8 into string
 * (ie. base node's public key)
 * @param {string} data - array of U8 as a string
 */
export const convertU8ToString = (data: string) => {
  try {
    if (!data || data === '[]') {
      return ''
    }
    const parsed = data.replace('[', '').replace(']', '').split(',')
    return parsed.map(c => String.fromCharCode(Number(c))).join('')
  } catch (_) {
    return ''
  }
}

/**
 * Convert Bytes array into the Hex (as string)
 * @param {number[]} bytes - U8 bytes array
 */
export const bytesToHex = (bytes: number[]): string => {
  const hex = []
  for (let i = 0; i < bytes.length; i++) {
    const current = bytes[i] < 0 ? bytes[i] + 256 : bytes[i]
    hex.push((current >>> 4).toString(16))
    hex.push((current & 0xf).toString(16))
  }
  return hex.join('')
}

/**
 * Convert snake case to the camel case
 * @param {string} text - text to convert
 */
export const snakeCaseToCamelCase = (text: string) => {
  return text.replace(/[^a-zA-Z0-9]+(.)/g, (m, chr) => chr.toUpperCase())
}
