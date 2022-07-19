import { useEffect } from 'react'

import { selectDockerTBotQueue } from './store/dockerImages/selectors'
import { useAppSelector } from './store/hooks'
import { selectTBotQueue } from './store/tbot/selectors'
import { tbotactions } from './store/tbot'
import MessagesConfig from './config/helpMessagesConfig'
import { AppDispatch } from './store'

export const useDockerTBotQueue = ({ dispatch }: { dispatch: AppDispatch }) => {
  const dockerTBotQueue = useAppSelector(selectDockerTBotQueue)
  const tBotQueue = useAppSelector(selectTBotQueue)

  useEffect(() => {
    if (tBotQueue?.length === 0 && dockerTBotQueue?.length > 0) {
      dispatch(tbotactions.push(MessagesConfig.NewDockerImageToDownload))
    }
  }, [dockerTBotQueue, tBotQueue])
}
