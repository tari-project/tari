import usePerformanceStats from './usePerformanceStats'

const PerformanceContainer = () => {
  const { cpu, memory, network } = usePerformanceStats()

  // RENDER DELIGHTFUL CHARTS
  return (
    <div style={{ height: '500px', overflow: 'auto' }}>
      <pre style={{ color: 'white' }}>
        {JSON.stringify({ cpu, memory, network }, null, 2)}
      </pre>
    </div>
  )
}

export default PerformanceContainer
