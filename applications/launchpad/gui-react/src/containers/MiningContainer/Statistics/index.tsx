const Statistics = ({ onClose }: { onClose: () => void }) => {
  return (
    <p>
      hello world - statistics <button onClick={onClose}>close</button>
    </p>
  )
}

const StatisticsContainer = ({ onClose }: { onClose: () => void }) => {
  return <Statistics onClose={onClose} />
}

const StatisticsWrapper = ({
  open,
  onClose,
}: {
  open: boolean
  onClose: () => void
}) => {
  if (!open) {
    return null
  }

  return <StatisticsContainer onClose={onClose} />
}

export default StatisticsWrapper
