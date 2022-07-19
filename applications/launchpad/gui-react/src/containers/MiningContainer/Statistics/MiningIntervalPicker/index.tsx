import { useState, useEffect } from 'react'

import useTransactionsRepository from '../../../../persistence/transactionsRepository'

import { MiningIntervalPickerComponentProps } from './types'
import MiningIntervalPickerComponent from './MiningIntervalPickerComponent'

/**
 * @name MiningIntervalPicker
 * @description controlled component that allows user to change currently picked interval - if it is a month, user iterates over months, if it is a year, years
 *
 * @prop {Date} value - value of current interval picked
 * @prop {MiningStatisticsInterval} interval - what intervals we are showing (month of year)
 * @prop {(d: Date) => void} onChange - callback called with new values when user iterates over intervals
 */
const MiningIntervalPicker = ({
  value,
  interval,
  onChange,
}: Omit<MiningIntervalPickerComponentProps, 'dataFrom' | 'dataTo'>) => {
  const transactionsRepository = useTransactionsRepository()

  const [{ from: dataFrom, to: dataTo }, setDates] = useState<{
    from: Date
    to: Date
  }>({ from: new Date(), to: new Date() })
  useEffect(() => {
    const getData = async () => {
      const dates = await transactionsRepository.getMinedTransactionsDataSpan()

      setDates(dates)
    }
    getData()
  }, [])

  return (
    <MiningIntervalPickerComponent
      value={value}
      interval={interval}
      onChange={onChange}
      dataFrom={dataFrom}
      dataTo={dataTo}
    />
  )
}

export default MiningIntervalPicker
