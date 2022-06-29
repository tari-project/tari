import { useState, useMemo } from 'react'

import { useAppSelector, useAppDispatch } from '../../../../store/hooks'
import { selectContainersStatusesWithStats } from '../../../../store/containers/selectors'
import { Container, ContainerId } from '../../../../store/containers/types'
import { actions } from '../../../../store/containers'
import Alert from '../../../../components/Alert'

import Containers from './Containers'

const ContainersContainer = () => {
  const [error, setError] = useState('')

  const dispatch = useAppDispatch()
  const containerStatuses = useAppSelector(selectContainersStatusesWithStats)
  const containers = useMemo(
    () =>
      containerStatuses.map(
        ({ container, imageName, displayName, status }) => ({
          id: status.id,
          container: container as Container,
          imageName,
          displayName,
          error: status.error,
          cpu: status.stats.cpu,
          memory: status.stats.memory,
          pending: status.pending,
          running: status.running,
        }),
      ),
    [containerStatuses],
  )

  const start = async (container: Container) => {
    try {
      await dispatch(actions.start({ container: container })).unwrap()
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
    } catch (e: any) {
      setError(e.toString())
    }
  }
  const stop = async (containerId: ContainerId) => {
    try {
      await dispatch(actions.stop(containerId)).unwrap()
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
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
