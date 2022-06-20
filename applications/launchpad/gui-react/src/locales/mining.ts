const translations = {
  minedInLastSession: 'mined in last session',
  setUpTariWalletSubmitBtn: 'Set up Tari Wallet & start mining',
  readyToMiningText: 'Everything is set. You’re ready to go!',
  headerTips: {
    oneStepAway: 'You are one step away from starting mining.',
    oneClickAway: 'You are one click away from starting mining.',
    continueMining:
      'Keep on going. You are one click away from starting mining.',
    runningOn: 'Awesome! Tari Mining is on.',
    wantToKnowMore: 'Want to know more',
  },
  actions: {
    startMining: 'Start mining',
    setupAndStartMining: 'Set up & start mining',
  },
  viewActions: {
    setUpMiningHours: 'Set up mining hours',
    miningSettings: 'Mining settings',
    statistics: 'Statistics',
  },
  placeholders: {
    statusUnknown: 'The node status is unknown.',
    statusBlocked: 'The node cannot be started.',
    statusSetupRequired: 'The node requires further configuration.',
    statusError:
      'TBD: Something went wrong with this or one of the dependent containers. Show alert like in the Expert View?',
  },
  setup: {
    description: 'If you want to start merged mining you need to',
    descriptionBold: 'set up your Monero address first.',
    addressPlaceholder: 'Set your Monero wallet address',
    formDescription:
      'This is the address to which the Monero coins you earn will be sent. Make sure it is correct as you might accidentally give a generous gift to a stranger. 😅',
  },
  scheduling: {
    title: 'Mining schedules',
    launchpadOpen:
      'Tari Launchpad must be open at the scheduled hours for mining to start.',
    noSchedules: 'No mining schedule has been set up yet',
    add: 'Add schedule',
    removeSchedule: 'Remove schedule',
    ops: 'Ops!',
    error_miningEndsBeforeItStarts:
      /* eslint-disable-next-line quotes */
      "I guess you need to correct the hours because mining can't stop before it even starts",
    error_miningEndsWhenItStarts:
      /* eslint-disable-next-line quotes */
      "I guess you need to correct the hours because mining can't stop exactly when it starts",
    error_miningInThePast:
      /* eslint-disable-next-line quotes */
      "I guess you need to correct the selected date because we can't mine in the past",
  },
  statistics: {
    title: 'Mined coins',
    intervals: {
      all: 'All',
      monthly: 'Monthly',
      yearly: 'Yearly',
    },
    deltas: {
      yearly: 'vs last year',
      monthly: 'vs last month',
      // this is required otherwise accessing this through MiningStatisticsInterval union type breaks
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
    } as any,
  },
  settings: {
    title: 'Mining Settings',
    moneroAddressLabel: 'Monero mining address',
    moneroAddressDesc1:
      'This is the address to which the Monero coins you earn will be sent.',
    moneroAddressDesc2:
      'You need to provide a Monero address to be able to start Merged mining.',
    threadsLabel: 'SHA3 threads',
    moneroUrlLabel: 'Monero node URL',
    addNextUrl: 'Add next URL',
    wrongUrlFormat: 'Oops! This is not a valid URL',
    moneroUrlPlaceholder: 'Set URL address',
    moneroNodeAuthLabel: 'Monero node authentication',
    moneroAuthFormTitle: 'Apply Authentication',
    moneroAuthFormDesc:
      'To ensure that Tari Launchpad communicates with the external Monero node you chose please enter valid data that secures it.',
    moneroAuthApplied: 'Monero node authentication applied',
    authUsernameLabel: 'Username (optional)',
    authUsernamePlaceholder: 'Username you log in to the Monero node',
    authPasswordLabel: 'Password (optional)',
    authPasswordPlaceholder: 'Password you log in to the Monero node',
  },
}

export default translations
