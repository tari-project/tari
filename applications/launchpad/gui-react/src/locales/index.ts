/**
 * I18n
 *
 * @example
 * import t from './locales'
 *
 * <p>{t.common.nouns.wallet}</p>
 */
import commonI18n from './common'

import footerI18n from './footer'
import miningI18n from './mining'
import baseNodeI18n from './baseNode'
import walletI18n from './wallet'
import expertViewI18n from './expertView'
import walletPasswordWizardI18n from './walletPasswordWizard'
import cryptoMiningHelpIl8n from './cryptoMiningHelp'
import mergedMiningHelpIl8n from './mergedMiningHelp'
import settingsIl8n from './settings'

const translations = {
  common: commonI18n,
  footer: footerI18n,
  mining: miningI18n,
  baseNode: baseNodeI18n,
  wallet: walletI18n,
  expertView: expertViewI18n,
  walletPasswordWizard: walletPasswordWizardI18n,
  cryptoMiningHelp: cryptoMiningHelpIl8n,
  mergedMiningHelp: mergedMiningHelpIl8n,
  settings: settingsIl8n,
}

export default translations
