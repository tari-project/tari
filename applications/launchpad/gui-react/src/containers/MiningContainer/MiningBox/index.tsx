import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTheme } from 'styled-components'
import deepmerge from 'deepmerge'

import Button from '../../../components/Button'
import CoinsList from '../../../components/CoinsList'
import NodeBox, { NodeBoxContentPlaceholder } from '../../../components/NodeBox'
import Text from '../../../components/Text'

import { useAppDispatch } from '../../../store/hooks'

import { actions } from '../../../store/mining'
import { MiningActionReason, MiningSession } from '../../../store/mining/types'

import t from '../../../locales'

import {
  MiningBoxProps,
  MiningBoxStatus,
  MiningCoinIconProp,
  NodeBoxStatusConfig,
} from './types'
import { MiningBoxContent, NodeIcons } from './styles'
import RunningButton from '../../../components/RunningButton'
import { tbotactions } from '../../../store/tbot'
import useWithPasswordPrompt from '../../../containers/PasswordPrompt/useWithPasswordPrompt'
import Alert from '../../../components/Alert'

const parseLastSessionToCoins = (
  lastSession: MiningSession | undefined,
  theCurrentStatus?: MiningBoxStatus,
  icons?: MiningCoinIconProp[],
) => {
  if (lastSession && lastSession.total) {
    const anyNonZeroCoin = Object.entries(lastSession.total).some(
      c => Number(c[1]) !== 0,
    )
    return Object.keys(lastSession.total).map(coin => ({
      unit: coin,
      amount:
        lastSession.total && lastSession.total[coin]
          ? lastSession.total[coin]
          : '0',
      loading: !anyNonZeroCoin && theCurrentStatus === MiningBoxStatus.Running,
      suffixText: lastSession.finishedAt ? t.mining.minedInLastSession : '',
      icon: icons?.find(i => i.coin === coin)?.component,
    }))
  }

  return []
}

/**
 * Generic component providing NodeBox-based UI for mining containers.
 *
 * The box can be in one of few states: paused, running, error, setup_required or custom.
 * It is evaluated from the 'containersState' based on the running, error, pending etc. fields.
 * It also provides generic start and pause actions that dispatch the mining's start/stop actions.
 *
 * The component tries to resolve the state and what need to be rendered by itself, but in some cases,
 * some customisation may be required, ie. when the node has to be configured, or we want style it differently.
 * In such case, you can:
 * a) provide 'children' (React component) and it will replace the generic content of the box.
 * b) statuses - changes the styling of the box and its sub-components
 * c) currentStatus - when more advanced logic needs to be applied.
 *
 * The general approach is:
 * 1. Create parent container for specific node (ie. Tari Mining)
 * 2. Import and render this MiningBox Container with minimal config
 * 3. Add in parent container any custom logic that will evaluate the correct status. If it's needed to provide
 *    custom component and logic for a given status, push children component (it will override generic component and behaviour).
 *
 * @param {MiningNodeType} node - ie. tari, merged
 * @param {Partial<{[key in MiningBoxStatus]: NodeBoxStatusConfig}>} [statuses] - the optional config overriding specific states.
 * @param {MiningBoxStatus} [currentStatus] - overrides the current status (ie. force setup_required)
 * @param {ReactNode[]} [icons] - right-side icons
 * @param {string} [testId] - custom test id
 * @param {MiningNodeState} [nodeState] - the node state from Redux's mining
 * @param {MiningContainersState} [containersState] - the containers from Redux's mining
 * @param {ReactNode} [children] - component overriding the generic one composed by this container for a given status.
 * @param {string[]} [helpMessages] - help prompt messages
 */

const MiningBox = ({
  node,
  icons,
  statuses,
  currentStatus,
  children,
  testId = 'mining-box-cmp',
  nodeState,
  containersState,
  helpMessages,
  requiredAuthentication,
}: MiningBoxProps) => {
  const dispatch = useAppDispatch()
  const theme = useTheme()

  const [theCurrentStatus, setTheCurrentStatus] = useState(currentStatus)
  const [showAlert, setShowAlert] = useState<string | undefined>(undefined)

  const startMining = useCallback(() => {
    dispatch(
      actions.startMiningNode({
        node,
        reason: MiningActionReason.Manual,
      }),
    )
  }, [dispatch, node])

  const startMiningWithPasswordEnsured = useWithPasswordPrompt(
    startMining,
    requiredAuthentication || { wallet: false, monero: false },
  )

  const lastSession = nodeState.session

  useEffect(() => {
    if (containersState.error) {
      if (typeof containersState.error === 'string') {
        setShowAlert(containersState.error)
      } else {
        setShowAlert(containersState.error[0]?.error)
      }
    }

    if (currentStatus) {
      return
    }

    if (containersState.running) {
      setTheCurrentStatus(MiningBoxStatus.Running)
    } else if (!lastSession) {
      setTheCurrentStatus(MiningBoxStatus.PausedNoSession)
    } else {
      setTheCurrentStatus(MiningBoxStatus.Paused)
    }
  }, [containersState, currentStatus])

  const coins = parseLastSessionToCoins(lastSession, theCurrentStatus, icons)

  // Is there any ongoing action, so the buttons should be disabled?
  const disableActions = containersState.pending

  const defaultConfig: NodeBoxStatusConfig = {
    title: `${node.substring(0, 1).toUpperCase() + node.substring(1)} ${
      t.common.nouns.mining
    }`,
    boxStyle: {
      color: theme.primary,
      background: theme.nodeBackground,
    },
    titleStyle: {
      color: theme.helpTipText,
    },
    contentStyle: {
      color: theme.secondary,
    },
    icon: {
      color: theme.nodeLightIcon,
    },
    helpSvgGradient: false,
  }

  const defaultStates: Partial<{
    [key in MiningBoxStatus]: NodeBoxStatusConfig
  }> = {
    [MiningBoxStatus.SetupRequired]: {
      tag: {
        content: t.common.phrases.startHere,
      },
      helpSvgGradient: true,
    },
    [MiningBoxStatus.Paused]: {
      tag: {
        content: t.common.adjectives.paused,
        type: 'light',
      },
      titleStyle: {
        color: theme.helpTipText,
      },
      boxStyle: {
        background: theme.nodeBackground,
      },
      icon: {
        color: theme.nodeLightIcon,
      },
    },
    [MiningBoxStatus.PausedNoSession]: {
      tag: {
        content: t.common.phrases.startHere,
      },
      helpSvgGradient: true,
    },
    [MiningBoxStatus.Running]: {
      tag: {
        content: t.common.adjectives.running,
        type: 'running',
      },
      boxStyle: {
        background: theme.tariGradient,
      },
      titleStyle: {
        color: theme.inverted.primary,
      },
      contentStyle: {
        color: theme.inverted.primary,
      },
      icon: {
        color: theme.accentDark,
      },
    },
    [MiningBoxStatus.Error]: {
      tag: {
        content: t.common.nouns.problem,
        type: 'warning',
      },
    },
  }

  const currentState = useMemo(() => {
    const fromDefaultState = theCurrentStatus
      ? defaultStates[theCurrentStatus]
      : {}
    const fromProps =
      theCurrentStatus && statuses && statuses[theCurrentStatus]
        ? statuses[theCurrentStatus]
        : {}
    return deepmerge.all([
      defaultConfig,
      fromDefaultState || {},
      fromProps || {},
    ]) as NodeBoxStatusConfig
  }, [theCurrentStatus, nodeState, statuses])

  const componentForCurrentStatus = () => {
    if (children) {
      return children
    }

    switch (theCurrentStatus) {
      case MiningBoxStatus.SetupRequired:
        return (
          <NodeBoxContentPlaceholder testId='node-box-placeholder--setup-required'>
            {t.mining.placeholders.statusSetupRequired}
          </NodeBoxContentPlaceholder>
        )
      case MiningBoxStatus.Error:
        return (
          <NodeBoxContentPlaceholder testId='node-box-placeholder--error'>
            <MiningBoxContent>
              {coins ? <CoinsList coins={coins} /> : null}
              <Button
                onClick={startMiningWithPasswordEnsured}
                disabled={disableActions}
                loading={disableActions}
                testId={`${node}-run-btn`}
              >
                {t.mining.actions.startMining}
              </Button>
              <div>{t.mining.placeholders.statusError}</div>
            </MiningBoxContent>
          </NodeBoxContentPlaceholder>
        )
      case MiningBoxStatus.PausedNoSession:
        return (
          <MiningBoxContent data-testid='mining-box-paused-content'>
            <Text>{t.mining.readyToMiningText}</Text>
            <Button
              onClick={startMiningWithPasswordEnsured}
              disabled={disableActions}
              loading={disableActions}
              testId={`${node}-run-btn`}
            >
              {t.mining.actions.startMining}
            </Button>
          </MiningBoxContent>
        )
      case MiningBoxStatus.Paused:
        return (
          <MiningBoxContent data-testid='mining-box-paused-content'>
            {coins ? (
              <CoinsList coins={coins} color={theme.nodeWarningText} />
            ) : null}
            <Button
              onClick={startMiningWithPasswordEnsured}
              disabled={disableActions}
              loading={disableActions}
              testId={`${node}-run-btn`}
            >
              {t.mining.actions.startMining}
            </Button>
          </MiningBoxContent>
        )
      case MiningBoxStatus.Running:
        return (
          <MiningBoxContent data-testid='mining-box-running-content'>
            {coins ? (
              <CoinsList
                coins={coins}
                color={theme.textSecondary}
                unitsColor={theme.textSecondary}
                showSymbols
              />
            ) : null}
            <RunningButton
              onClick={() =>
                dispatch(
                  actions.stopMiningNode({
                    node,
                    reason: MiningActionReason.Manual,
                  }),
                )
              }
              startedAt={
                nodeState?.session?.finishedAt
                  ? Number(Date.now())
                  : Number(nodeState?.session?.startedAt) || Number(Date.now())
              }
              testId={`${node}-pause-btn`}
            />
          </MiningBoxContent>
        )
    }
  }

  const content = componentForCurrentStatus()

  let helpPromptClick

  if (helpMessages) {
    helpPromptClick = () => {
      return dispatch(tbotactions.push(helpMessages))
    }
  }
  return (
    <>
      <NodeBox
        title={currentState.title}
        tag={currentState.tag}
        style={{ position: 'relative', ...currentState.boxStyle }}
        titleStyle={currentState.titleStyle}
        contentStyle={currentState.contentStyle}
        onHelpPromptClick={helpPromptClick}
        helpSvgGradient={currentState.helpSvgGradient}
        testId={testId}
      >
        {icons && icons.length > 0 ? (
          <NodeIcons
            $color={currentState.icon?.color || theme.backgroundSecondary}
          >
            {icons.map(icon => icon.component)}
          </NodeIcons>
        ) : null}
        {content}
      </NodeBox>
      <Alert
        open={Boolean(showAlert)}
        content={showAlert}
        onClose={() => setShowAlert(undefined)}
      />
    </>
  )
}

export default MiningBox
