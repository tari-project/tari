export default {
  discardChanges: 'Discard changes?',
  discardChangesDesc: 'Closing this window will cancel any unsaved changes',
  closeAndDiscard: 'Close & Discard',
  selectTheme: 'Select theme',
  security: {
    title: 'Security Settings',
    showRecoveryPhrase: 'Show recovery phrase',
    backupRecoveryPhrase: 'Backup Recovery Phrase',
    backupRecoveryPhraseExplanation: {
      part1:
        // eslint-disable-next-line quotes
        "It's absolutely crucial to keep your Backup Recovery Phrase secret and safe. Anybody who has access to it can steal your Tari funds.",
      part2:
        'Do not store you Backup Recovery Phrase on your smartphone, computer or any other device that can connect to the internet.',
      part3:
        'Print or rewrite the blank Recovery Sheet, fill it with the Backup Recovery Phrase (24 words shown in next steps) and then store it in a secure place. Do not take pictures of the filled Recovery Sheet.',
    },
    printRecoverySheet: 'Print Recovery Sheet',
    writeDownRecoveryPhraseInstructions:
      'Write down the following 24 words. It is crucial you write them down in the exact order as shown.',
    prev4Words: 'Previous 4 words',
    next4Words: 'Next 4 words',
    backToRecoveryPhrase: 'Back to recovery phrase',
    submitAndFinish: 'Submit & finish',
    phraseConfirmError:
      // eslint-disable-next-line quotes
      'Oh snap! Entered words do not match the Backup Recovery Phrase.',
    confirmPhraseDesc:
      'Enter specific words according to their numbers on the Recovery Sheet.',
    couldNotGetSeedWords:
      'Could not create Backup Recovery Phrase. Make sure that the Wallet is running.',
    createRecoveryPhrase: 'Create recovery phrase',
    tab: {
      desc: 'Your Backup Recovery Phrase allows you to restore the wallet and access your funds when you:',
      list1: 'forgot your password,',
      list2: 'deleted your Tari wallet,',
      list3: 'computer has crashed.',
    },
    alreadyCreated: 'Already created',
  },
}
