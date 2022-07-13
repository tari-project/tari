import 'styled-components'

declare module '*.svg' {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const content: any
  export default content
}

declare module '*.otf'

declare module 'styled-components' {
  export interface DefaultTheme {
    spacing: (count?: number) => string
    spacingHorizontal: (count?: number) => string
    spacingVertical: (count?: number) => string
    borderRadius: (count?: number) => string
    tightBorderRadius: (count?: number) => string
    transparent: (color: string, transparency: number) => string
    tabsMarginRight: number
    primary: string
    secondary: string
    tertiary: string
    background: string
    backgroundSecondary: string
    backgroundImage: string
    accent: string
    accentDark: string
    accentMonero: string
    accentMerged: string
    accentMerged: string
    disabledText: string
    tariGradient: string
    tariTextGradient: string
    mergedGradient: string
    warningGradient: string
    greenMedium: string
    borderColor: string
    borderColorLight: string
    actionBackground: string
    resetBackground: string
    resetBackgroundHover: string
    moneroDark: string
    shadow40: string
    shadow24: string

    titleBar: string

    controlBackground: string
    infoTag: string
    infoText: string
    on: string
    onText: string
    onTextLight: string
    warning: string
    warningTag: string
    warningText: string
    warningDark: string
    success: string
    error: string
    expert: string
    expertText: string
    expertSecText: string
    lightTag: string
    lightTagText: string
    placeholderText: string
    textSecondary: string
    helpTipText: string
    runningTagBackground: string
    runningTagText: string
    dashboardRunningTagText: string
    dashboardRunningTagBackground: string
    switchBorder: string
    switchCircle: string
    switchController: string
    nodeBackground: string
    nodeLightIcon: string
    nodeSubHeading: string
    nodeWarningText: string
    calloutBackground: string
    inputPlaceholder: string
    disabledPrimaryButton: string
    disabledPrimaryButtonText: string
    baseNodeGradientStart: string
    baseNodeGradientEnd: string
    baseNodeRunningLabel: string
    baseNodeRunningTagBackground: string
    baseNodeRunningTagText: string
    selectBorderColor: string
    selectOptionHover: string
    walletSetupBorderColor: string
    walletCopyBoxBorder: string
    balanceBoxBorder: string
    walletBottomBox: string
    modalBackdrop: string
    settingsMenuItem: string
    settingsMenuItemActive: string
    settingsCopyBoxBackground: string
    scrollBarTrack: string
    scrollBarThumb: string
    scrollBarHover: string
    calendarText: string
    calendarTextSecondary: string
    calendarNumber: string
    tbotMessage: string
    tbotContentBackground: string
    buttonRadioBackground: string
    buttonRadioBorder: string

    inverted: {
      controlBackground: string
      primary: string
      secondary: string
      background: string
      backgroundSecondary: string
      backgroundImage: string
      accent: string
      accentSecondary: string
      accentDark: string
      accentMonero: string
      accentMerged: string
      disabledText: string
      tariGradient: string
      tariTextGradient: string
      warningGradient: string
      greenMedium: string
      resetBackground: string
      resetBackgroundHover: string
      moneroDark: string
      info: string
      infoText: string
      on: string
      onText: string
      onTextLight: string
      warning: string
      warningText: string
      warningDark: string
      success: string
      error: string
      expert: string
      expertText: string
      lightTag: string
      lightTagText: string
      expert: string
      expertText: string
      expertSecText: string
      switchBorder: string
      switchCircle: string
      switchController: string
      nodeBackground: string
      nodeLightIcon: string
      nodeSubHeading: string
      nodeWarningText: string
      calloutBackground: string
      inputPlaceholder: string
      disabledPrimaryButton: string
      disabledPrimaryButtonText: string
      baseNodeGradientStart: string
      baseNodeGradientEnd: string
      baseNodeRunningLabel: string
      baseNodeRunningTagBackground: string
      baseNodeRunningTagText: string
      selectBorderColor: string
      selectOptionHover: string
      walletSetupBorderColor: string
      walletCopyBoxBorder: string
      balanceBoxBorder: string
      walletBottomBox: string
      modalBackdrop: string
      settingsMenuItem: string
      settingsMenuItemActive: string
      settingsCopyBoxBackground: string
      scrollBarTrack: string
      scrollBarThumb: string
      scrollBarHover: string
      calendarText: string
      calendarTextSecondary: string
      calendarTextSecondary: styles.colors.light.graySecondary
      tbotMessage: string
      tbotContentBackground: string
      buttonRadioBackground: string
      buttonRadioBorder: string
    }
  }
}
