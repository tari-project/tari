/* eslint-disable react/jsx-key */
import { useEffect, useRef, useState } from 'react'
import { appWindow } from '@tauri-apps/api/window'

import Text from '../../Text'
import t from '../../../locales'
import Button from '../../Button'

import { useAppDispatch } from '../../../store/hooks'
import { setOnboardingComplete } from '../../../store/app'

import { actions as containersActions } from '../../../store/containers'
import {
  CalcRemainTimeCont,
  CalcRemainTimeContLoader,
  CtaButtonContainer,
  FlexContent,
  ProgressContainer,
  RemainingTime,
} from './styles'
import ProgressBar from '../../ProgressBar'
import { TBotMessage, TBotMessageHOCProps } from '../../TBot/TBotPrompt/types'
import { useTheme } from 'styled-components'
import { SyncType, useBaseNodeSync } from '../../../useBaseNodeSync'
import Loading from '../../Loading'
import { humanizeEstimatedTime } from '../../../utils/Format'

/**
 * Renders the progress bar and remaining time
 */
const Progress = ({
  progress,
  time,
  type,
}: {
  progress?: number
  time?: number
  type?: SyncType
}) => {
  const theme = useTheme()

  return (
    <ProgressContainer>
      <Text
        color={theme.accent}
        type='smallMedium'
        style={{
          alignSelf: 'flex-start',
          marginBottom: theme.spacingVertical(0.5),
        }}
      >
        {t.onboarding.lastSteps.blockchainIsSyncing}{' '}
        {type && type === 'Header' && '(1/2)'}
        {type && type === 'Block' && '(2/2)'}
      </Text>
      <ProgressBar value={progress || 0} />
      <RemainingTime>
        <Text type='smallMedium'>
          {time === undefined || type === 'Startup' ? (
            <CalcRemainTimeCont>
              <CalcRemainTimeContLoader>
                <Loading loading size='14px' />
              </CalcRemainTimeContLoader>
              <span>
                {t.common.phrases.calculatingTheRemainingTime}
                ...
              </span>
            </CalcRemainTimeCont>
          ) : (
            <>
              {humanizeEstimatedTime(time)}{' '}
              {t.common.adjectives.remaining.toLowerCase()}
            </>
          )}
        </Text>
      </RemainingTime>
    </ProgressContainer>
  )
}

/**
 * Renders the onboarding message running the blockchain sync
 */
export const BlockchainSyncStep = ({
  pushMessages,
  updateMessageBoxSize,
}: {
  pushMessages: (msgs: TBotMessage[]) => void
} & TBotMessageHOCProps) => {
  const dispatch = useAppDispatch()

  const contentRef = useRef<HTMLDivElement | null>(null)

  const [syncStarting, setSyncStarting] = useState(false)
  const [syncStarted, setSyncStarted] = useState(false)

  const pushErrorMessage = () => {
    pushMessages([
      {
        content: (
          <>
            <Text>{t.onboarding.lastSteps.syncError}</Text>
            <CtaButtonContainer>
              <Button
                variant='secondary'
                onClick={() => dispatch(setOnboardingComplete(true))}
              >
                {t.common.verbs.continue}
              </Button>
            </CtaButtonContainer>
          </>
        ),
        barFill: 0.875,
        noSkip: true,
        wait: 200,
      },
    ])
  }

  const startSync = async () => {
    setSyncStarting(true)
    pushMessages([
      {
        content: <Text>{t.onboarding.lastSteps.message2}</Text>,
        barFill: 0.875,
        noSkip: true,
      },
    ])
    try {
      await dispatch(
        containersActions.startRecipe({ containerName: 'base_node' }),
      ).unwrap()
      setSyncStarted(true)
    } catch (err) {
      try {
        await dispatch(
          containersActions.startRecipe({ containerName: 'base_node' }),
        ).unwrap()
        setSyncStarted(true)
      } catch (err2) {
        pushErrorMessage()
      }
    }
  }

  const baseNodeSyncProgress = useBaseNodeSync(syncStarted)

  useEffect(() => {
    if (syncStarted && updateMessageBoxSize && contentRef.current) {
      updateMessageBoxSize()
    }
  }, [syncStarted])

  const finishSyncing = () => {
    dispatch(setOnboardingComplete(true))
  }

  useEffect(() => {
    if (baseNodeSyncProgress.finished) {
      finishSyncing()
    }
  }, [baseNodeSyncProgress.finished])

  return (
    <FlexContent ref={contentRef}>
      <Text as='span' type='defaultHeavy'>
        {t.onboarding.lastSteps.message1} ✨💪
      </Text>

      {!syncStarting && (
        <CtaButtonContainer>
          <Button variant='primary' onClick={startSync}>
            {t.onboarding.dockerInstall.startSyncBtn}
          </Button>
        </CtaButtonContainer>
      )}

      {syncStarting && (
        <>
          <Progress
            progress={baseNodeSyncProgress.progress}
            time={baseNodeSyncProgress.remainingTime}
            type={baseNodeSyncProgress.syncType}
          />
          <CtaButtonContainer style={{ justifyContent: 'center' }}>
            <Button variant='secondary' onClick={() => appWindow.close()}>
              {t.common.verbs.cancel}
            </Button>
          </CtaButtonContainer>
        </>
      )}
    </FlexContent>
  )
}
