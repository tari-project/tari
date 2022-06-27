const translations = {
  title: 'Base Node',
  tari_network_label: 'Tari network',
  start: 'Start Base Node',
  unhealthy: {
    warning:
      'Not all containers required to run Base Node are in a healthy state.',
    ofTheRequired: 'of the required containers is running.',
    containers: 'Containers that are not running correctly:',
    checkTheirState: 'You can check their state in',
    bringItDown: 'or bring the service down entirely and start again.',
  },
  errors: {
    start: 'Base Node could not start.',
    stop: 'Base Node did not stop.',
  },
  settings: {
    title: 'Base Node Settings',
    rootFolder: 'Root folder',
    aurora: 'your Aurora app to the Base Node to increase the security',
  },
  helpMessages: {
    howItWorks: {
      tip: {
        text: 'Begin by starting the Base Node',
        cta: 'How it works',
      },
      allowsYou: 'Running Tari Base Node allows you to:',
      affordances: [
        'Mine Tari (XTR)',
        'Transact using the Tari Wallet',
        ' Query and analyze chain data using your local copy of the ledger',
      ],
      thankYou: '👊 Thank you for mining Tari Base Node.',
      yourContribution:
        'Every new node increases the size of the Tari network and contributes to network security.',
    },
  },
}

export default translations
