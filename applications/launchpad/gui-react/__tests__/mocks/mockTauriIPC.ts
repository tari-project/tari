// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import { mockIPC } from '@tauri-apps/api/mocks'

import { ServiceDescriptor } from '../../src/store/containers/types'

/**
 * Set of default values returned by `tauriIPCMock()`
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const defaultTauriMockValues: Record<string, any> = {
  os: {
    arch: 'x86_64',
    platform: 'darwin',
    ostype: 'Darwin',
  },
  window: {
    manage: {
      innerSize: {
        width: 1200,
        height: 800,
      },
    },
  },
}

/**
 * The Tauri IPC mock.
 *
 * It uses Tauri's mockIPC and returns the value set in the `props`.
 * If nothing found in `props`, it will return a value from `defaultTauriMockValues`.
 *
 * @param {Record<string, unknown>} props - pass the value you expect in tests
 *
 * @example
 * // Use default values:
 * tauriIPCMock()
 *
 * // Get given value from specific API module (ie. 'platform' from 'os' module)
 * tauriIPCMock({
 *   os: {
 *     platform: 'darwin',
 *   },
 * })
 */
export const tauriIPCMock = (props: Record<string, unknown> = {}) => {
  return mockIPC((cmd, args) => {
    switch (cmd) {
      case 'tauri':
        return tauriCmdMock(cmd, args, props)
      case 'invoke':
        return
      case 'start_service':
        return {
          id: `${args.serviceName}-id`,
          logEventsName: `tari://docker_log_${args.serviceName}`,
          statsEventsName: `tari://docker_stats_${args.serviceName}-id`,
          name: args.serviceName,
        } as ServiceDescriptor
      case 'stop_service':
        return true
      case 'image_info':
        return { imageInfo: [], serviceRecipes: [] }
      default:
        return
    }
  })
}

const tauriCmdMock = (
  _cmd: string,
  args: Record<string, unknown>,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  props: Record<string, any>,
) => {
  const tauriModule = (args?.__tauriModule as string)?.toLowerCase()
  const messageCmd = (args?.message as { cmd?: string })?.cmd?.toLowerCase()
  const messageDataCmd = (
    args?.message as { cmd?: string; data?: { label?: string; cmd?: string } }
  )?.data?.cmd

  if (tauriModule && messageCmd) {
    if (
      props &&
      Object.keys(props).includes(tauriModule) &&
      Object.keys(props[tauriModule]).includes(messageCmd)
    ) {
      if (
        messageDataCmd &&
        Object.keys(props[tauriModule][messageCmd]).includes(messageDataCmd)
      ) {
        return props[tauriModule][messageCmd][messageDataCmd]
      }

      return props[tauriModule][messageCmd]
    } else if (
      Object.keys(defaultTauriMockValues).includes(tauriModule) &&
      Object.keys(defaultTauriMockValues[tauriModule]).includes(messageCmd)
    ) {
      if (
        messageDataCmd &&
        Object.keys(defaultTauriMockValues[tauriModule][messageCmd]).includes(
          messageDataCmd,
        )
      ) {
        return defaultTauriMockValues[tauriModule][messageCmd][messageDataCmd]
      }

      return defaultTauriMockValues[tauriModule][messageCmd]
    }
  }

  return
}
