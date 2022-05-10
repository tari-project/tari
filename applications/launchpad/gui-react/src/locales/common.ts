import { Container } from '../store/containers/types'

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
  containers: {
    [Container.Tor]: 'Tor',
    [Container.BaseNode]: 'Base Node',
    [Container.Wallet]: 'Wallet',
    [Container.SHA3Miner]: 'SHA3 miner',
    [Container.MMProxy]: 'Merge miner proxy',
    [Container.XMrig]: 'xmrig',
    [Container.Monerod]: 'monerod',
    [Container.Frontail]: 'frontail',
  },
}

export default translations
