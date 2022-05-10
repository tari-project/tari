import { Service } from '../store/services/types'

const translations: { [key: string]: { [key: string]: string } } = {
  verbs: {
    accept: 'Accept',
    cancel: 'Cancel',
    stop: 'Stop',
    start: 'Start',
    pause: 'Pause',
    continue: 'Continue',
  },
  nouns: {
    baseNode: 'Base Node',
    mining: 'Mining',
    problem: 'Problem',
    settings: 'Settings',
    wallet: 'Wallet',
    performance: 'Performance',
    containers: 'Containers',
    logs: 'Logs',
    cpu: 'CPU',
    memory: 'Memory',
  },
  adjectives: {
    running: 'Running',
    paused: 'Paused',
    copied: 'Copied',
  },
  conjunctions: {
    or: 'or',
  },
  phrases: {
    actionRequired: 'Action required',
    startHere: 'Start here',
  },
  services: {
    [Service.Tor]: 'Tor',
    [Service.BaseNode]: 'Base Node',
    [Service.Wallet]: 'Wallet',
    [Service.SHA3Miner]: 'SHA3 miner',
    [Service.MMProxy]: 'Merge miner proxy',
    [Service.XMrig]: 'xmrig',
    [Service.Monerod]: 'monerod',
    [Service.Frontail]: 'frontail',
  },
}

export default translations
