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
import cryptoMiningHelpI18n from './cryptoMiningHelp'
import mergedMiningHelpI18n from './mergedMiningHelp'
import settingsI18n from './settings'
import onboardingI18n from './onboarding'
import dockerI18n from './docker'
import passwordPromptI18n from './passwordPrompt'
import onlineIl8n from './online'

const translations = {
  common: commonI18n,
  footer: footerI18n,
  mining: miningI18n,
  baseNode: baseNodeI18n,
  wallet: walletI18n,
  expertView: expertViewI18n,
  walletPasswordWizard: walletPasswordWizardI18n,
  cryptoMiningHelp: cryptoMiningHelpI18n,
  mergedMiningHelp: mergedMiningHelpI18n,
  settings: settingsI18n,
  onboarding: onboardingI18n,
  docker: dockerI18n,
  passwordPrompt: passwordPromptI18n,
  online: onlineIl8n,
}

export default translations
