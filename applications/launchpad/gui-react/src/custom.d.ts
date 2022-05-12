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
    accentMerged: string
    disabledText: string
    tariGradient: string
    mergedGradient: string
    warningGradient: string
    borderColor: string
    borderColorLight: string
    actionBackground: string
    resetBackground: string
    resetBackgroundHover: string
    shadow: string
    shadow2: string

    titleBar: string

    controlBackground: string
    info: string
    infoText: string
    on: string
    onText: string
    onTextLight: string
    warning: string
    warningText: string
    warningDark: string
    expert: string
    expertText: string
    lightTag: string
    lightTagText: string
    placeholderText: string

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
      accentMerged: string
      disabledText: string
      tariGradient: string
      warningGradient: string
      resetBackground: string
      resetBackgroundHover: string
      info: string
      infoText: string
      on: string
      onText: string
      onTextLight: string
      warning: string
      warningText: string
      warningDark: string
      expert: string
      expertText: string
      lightTag: string
      lightTagText: string
    }
  }
}
