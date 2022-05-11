import { useState, useMemo } from 'react'

import { useAppSelector, useAppDispatch } from '../../../../store/hooks'
import { selectContainersStatuses } from '../../../../store/containers/selectors'
import { Container, ContainerId } from '../../../../store/containers/types'
import { actions } from '../../../../store/containers'
import Alert from '../../../../components/Alert'

import Containers from './Containers'

const ContainersContainer = () => {
  const [error, setError] = useState('')

  const dispatch = useAppDispatch()
  const containerStatuses = useAppSelector(selectContainersStatuses)
  const containers = useMemo(
    () =>
      containerStatuses.map(({ container, status }) => ({
        id: status.id,
        container: container as Container,
        error: status.error,
        cpu: status.stats.cpu,
        memory: status.stats.memory,
        pending: status.pending,
        running: status.running,
      })),
    [containerStatuses],
  )

  const start = async (container: Container) => {
    try {
      await dispatch(actions.start(container)).unwrap()
    } catch (e: any) {
      setError(e.toString())
    }
  }
  const stop = async (containerId: ContainerId) => {
    try {
      await dispatch(actions.stop(containerId)).unwrap()
    } catch (e: any) {
      setError(e.toString())
    }
  }

  return (
    <>
      <Containers containers={containers} stop={stop} start={start} />
      <Alert
        title='Error'
        open={Boolean(error)}
        onClose={() => setError('')}
        content={error}
      />
    </>
  )
}

export default ContainersContainer
