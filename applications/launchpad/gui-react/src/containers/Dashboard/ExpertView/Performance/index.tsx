import { useEffect, useRef, useState, useMemo } from 'react'
import { useTheme } from 'styled-components'

import t from '../../../../locales'

import PerformanceChart from './PerformanceChart'

/**
 * @name PerformanceContainer
 * @description container component for performance statistics, renders filtering controls and performance charts
 * manages refresh rate and synchronizes refresh ticks for all charts
 * delegates chart rendering etc to other components
 *
 */
const PerformanceContainer = () => {
  const theme = useTheme()

  const last = 30 * 60 * 1000
  const refreshRate = 1000
  const [now, setNow] = useState(() => {
    const n = new Date()
    n.setMilliseconds(0)

    return n
  })
  const from = useMemo(() => new Date(now.getTime() - last), [now])
  const intervalRef = useRef<ReturnType<typeof setInterval> | undefined>()
  const [refreshEnabled, setRefreshEnabled] = useState<{
    cpu: boolean
    memory: boolean
    network: boolean
  }>({
    cpu: true,
    memory: true,
    network: true,
  })

  useEffect(() => {
    intervalRef.current = setInterval(() => {
      const n = new Date()
      n.setMilliseconds(0)
      setNow(n)
    }, refreshRate)

    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    return () => clearInterval(intervalRef.current!)
  }, [])

  return (
    <div style={{ overflow: 'auto' }}>
      <PerformanceChart
        enabled={refreshEnabled.cpu}
        extractor={({ timestamp, cpu }) => ({
          timestamp,
          value: cpu,
        })}
        percentageValues
        from={from}
        to={now}
        title={t.common.nouns.cpu}
        onUserInteraction={({ interacting }) => {
          setRefreshEnabled(a => ({
            ...a,
            cpu: !interacting,
          }))
        }}
        style={{ marginTop: theme.spacing() }}
        chartHeight={175}
      />
      <PerformanceChart
        enabled={refreshEnabled.memory}
        extractor={({ timestamp, memory }) => ({
          timestamp,
          value: memory,
        })}
        unit={t.common.units.mib}
        from={from}
        to={now}
        title={t.expertView.performance.memoryChartTitle}
        onUserInteraction={({ interacting }) => {
          setRefreshEnabled(a => ({
            ...a,
            memory: !interacting,
          }))
        }}
        style={{ marginTop: theme.spacing() }}
        chartHeight={175}
      />
      <PerformanceChart
        enabled={refreshEnabled.network}
        extractor={({ timestamp, download }) => ({
          timestamp,
          value: download / (1024 * 1024),
        })}
        unit={t.common.units.mib}
        from={from}
        to={now}
        title={t.expertView.performance.networkChartTitle}
        onUserInteraction={({ interacting }) => {
          setRefreshEnabled(a => ({
            ...a,
            network: !interacting,
          }))
        }}
        style={{ marginTop: theme.spacing() }}
        chartHeight={175}
      />
    </div>
  )
}

export default PerformanceContainer
