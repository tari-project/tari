/* eslint-disable quotes */
const translations = {
  setUpTariWalletTitle: 'Tari Wallet',
  setUpTariWalletSubmitBtn: 'Set up Tari Wallet',
  recentTransactions: 'Recent transactions',
  seeAllHistory: 'See all history',
  closeAllHistory: 'Close all history',
  theEnteredPasswordIsIncorrect: 'The entered password is incorrect',
  password: {
    title: 'Enter Password',
    cta: 'to unlock your wallet:',
    placeholderCta: 'Enter password to Tari Wallet',
  },
  wallet: {
    title: 'Tari Wallet',
    walletId: 'Tari Wallet ID',
    address: 'address',
  },
  balance: {
    title: 'Balance',
    available: 'Available to send',
    sendCta: 'Send funds',
  },
  transactions: {
    youReceivedTariFrom: 'You received Tari from',
    youSentTariTo: 'You sent Tari to',
    youEarnedTari: 'You earned Tari',
  },
  transaction: {
    transactionFee: 'Transaction fee',
    searchingForRecipient: 'Searching for the recipient on Tari network...',
    completingFinalProcessing: 'Completing final processing',
    completingDescription:
      'It takes usually up to 2 minutes to complete the operation.',
    transactionPending: 'Transaction pending',
    transactionPendingDesc1:
      'It appears that the recipient is not online at the moment and this is necessary to successfully complete the transaction.',
    transactionPendingDesc2:
      'If the recipient does not respond within the next 3 days, the transaction will be automatically canceled and the funds will be returned to your Tari Wallet.',
    form: {
      recipientIdAddress: 'Recipient ID (address)',
      recipientIdPlacehoder: 'Enter address of XTR Wallet to send the funds to',
      messageOptional: 'Message (optional)',
      messagePlaceholder: 'Save Tari coins and spend them wisely! ',
      sendFunds: 'Send funds',
    },
    errors: {
      exceedsAvailableAndFee: 'Available funds and fee are exceeded',
      messageIsTooLong: 'Message is too long',
      recipientIdError: 'The address must be at least 12 characters',
    },
  },
  settings: {
    title: 'Wallet Settings',
    explanations: {
      storage: `Mined Tari is stored in Launchpad's wallet.`,
      send: 'Send funds to wallet of your choice',
      try: 'try',
      aurora: 'Tari Aurora',
      itsGreat: `it's great!`,
      extendedFunctionality:
        'and enjoy extended functionality (including payment requests, recurring payments, ecommerce payments and more).',
      convert: 'To do this, you may need to convert the ID to emoji format.',
    },
  },
  helpMessages: {
    howItWorks: {
      tip: {
        text: 'Your Tari coins will be added here',
        cta: 'How it works',
      },
      title:
        'Tari wallet is a hardware app that allows you to manage your Tari balance.',
      message:
        'Only way to unlock access to your Tari wallet is by providing correct Tari wallet Password. Remember, unlike your social media account or bank account passwords, your Tari wallet password can never be recovered.',
    },
    whyBalanceDiffers: {
      title:
        'Why is the amount you can send different from your account balance?',
      message:
        'It is because the balance includes the pending transactions that you have made.',
    },
    noteAboutVerificationPeriod: {
      message:
        'Also note that Tari coins are not available immediately after being mined. They show up in your balance, but it takes usually several days (a few hundred blocks to be mined within the network) to verify this transaction.',
    },
    walletIdHelp: {
      bold: 'A wallet ID (also known as wallet identifier or wallet address) is like a bank account number.',
      regular:
        'Wallet ID is public and you can freely share it with others. That way, people can send you some Tari coins.',
    },
    transactionFee: {
      message:
        'The transaction fee is distributed to the thousands of computers (also known as “miners”) who ensure that your transactions are fast and secure.',
    },
  },
}

export default translations
