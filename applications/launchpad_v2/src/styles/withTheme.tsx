import { ComponentType } from 'react'

import { useTheme } from './useTheme'

export function withTheme(WrappeddComponent: ComponentType) {
  function WithTheme(props: any) {
    const theme = useTheme()

    return <WrappeddComponent {...props} theme={theme} />
  }

  WithTheme.displayName = `WithTheme(${getDisplayName(WrappeddComponent)})`

  return WithTheme
}

function getDisplayName(WrappedComponent: ComponentType) {
  return WrappedComponent.displayName || WrappedComponent.name || 'Component'
}
