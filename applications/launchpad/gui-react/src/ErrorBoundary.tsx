import { Component, CSSProperties, ErrorInfo, ReactNode } from 'react'
import { appWindow } from '@tauri-apps/api/window'

import linksConfig from './config/links'
import SvgTariLaunchpadLogo from './styles/Icons/TariLaunchpadLogo'
import SvgTBotBase from './styles/Icons/TBotBase'
import { hideSplashscreen } from './splashscreen'

interface ErrorBoundaryProps {
  children?: ReactNode
}

interface ErrorBoundaryState {
  error?: string
  showDetails: boolean
}

const styles = {
  windowContainer: {
    width: '100%',
    height: '100%',
    boxSizing: 'border-box',
    display: 'flex',
    flexDirection: 'column',
  },
  titlebar: {
    width: '100%',
    minHeight: 60,
    background: '#FAFAFA',
    padding: '16px 40px',
    boxSizing: 'border-box',
    display: 'flex',
    alignItems: 'center',
  },
  titleBarButtons: {
    display: 'flex',
    alignItems: 'center',
    marginRight: 32,
  },
  windowBtn: {
    margin: 0,
    padding: 2,
    width: 14,
    height: 14,
    borderRadius: '50%',
    boxShadow: 'none',
    border: '1px solid transparent',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    marginRight: 4,
    marginLeft: 4,
    cursor: 'pointer',
    boxSizing: 'border-box',
  },
  mainContainer: {
    display: 'flex',
    flex: 1,
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    padding: 40,
    boxSizing: 'border-box',
  },
  content: {
    width: '100%',
    maxWidth: 700,
    minHeight: 100,
    padding: 40,
    borderRadius: 6,
    border: '1px solid #EDECEE',
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    marginBottom: 32,
  },
  heading: {
    fontFamily: 'AvenirMedium',
    marginLeft: 16,
    marginTop: 8,
    marginBottom: 0,
  },
  mainMessageContainer: {
    paddingTop: 16,
    paddingBottom: 16,
  },
  text: {
    fontFamily: 'AvenirMedium',
    color: '#716A78',
    maxWidth: 600,
    lineHeight: '160%',
  },
  linkInText: {
    fontFamily: 'AvenirMedium',
    color: '#716A78',
  },
  buttons: {
    paddingTop: 16,
    paddingBottom: 16,
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    columnGap: 10,
  },
  button: {
    display: 'flex',
    flexDirection: 'row',
    justifyContent: 'center',
    alignItems: 'center',
    padding: '6px 24px',
    paddingTop: 8,
    gap: 10,
    background: 'linear-gradient(239.91deg, #9330FF 0%, #593A9B 131%)',
    borderRadius: 8,
    borderWidth: 0,
    fontFamily: 'AvenirHeavy',
    boxShadow: 'none',
    fontSize: 14,
    color: '#fff',
    cursor: 'pointer',
    minHeight: '38px',
  },
  detailsContainer: {
    display: 'none',
    background: '#FAFAFA',
    borderRadius: 6,
    border: '1px solid #EDECEE',
    padding: 12,
    maxHeight: 200,
    overflow: 'auto',
  },
  detailsText: {
    fontSize: 14,
    fontFamily: 'AvenirMedium',
    opacity: 0.7,
  },
}

/**
 * Catches the app exceptions.
 *
 * This implementation tries to NOT depend on anything, ie. styled-components and themes.
 */
class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state = {
    error: undefined,
    showDetails: false,
  }

  static getDerivedStateFromError(error: Error) {
    return { error: error.toString() }
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    // eslint-disable-next-line no-console
    console.error('Uncaught error:', error, errorInfo.componentStack)
    hideSplashscreen()
  }

  render() {
    if (this.state.error) {
      return (
        <div style={styles.windowContainer as CSSProperties}>
          <div style={styles.titlebar as CSSProperties} data-tauri-drag-region>
            <div style={styles.titleBarButtons}>
              <button
                style={{
                  ...(styles.windowBtn as CSSProperties),
                  borderColor: '#D24F43',
                  background: '#ED695E',
                }}
                onClick={() => appWindow.close()}
              >
                <svg width='6' height='6' viewBox='0 0 6 6' fill='none'>
                  <path
                    d='M4.76796 1.23242L1.23242 4.76796M4.76796 4.76796L1.23242 1.23242'
                    stroke='#BE493F'
                    strokeWidth='1.5'
                    strokeLinecap='round'
                  />
                </svg>
              </button>
              <button
                style={{
                  ...(styles.windowBtn as CSSProperties),
                  borderColor: '#D8A040',
                  background: '#F6BD50',
                }}
                onClick={() => appWindow.minimize()}
              >
                <svg
                  xmlns='http://www.w3.org/2000/svg'
                  width='10'
                  height='2'
                  viewBox='0 0 10 2'
                  fill='none'
                >
                  <path
                    d='M1 1H9'
                    stroke='#C2903A'
                    strokeWidth='1.5'
                    strokeLinecap='round'
                  />
                </svg>
              </button>
              <button
                style={{
                  ...(styles.windowBtn as CSSProperties),
                  borderColor: '#51A73E',
                  background: '#61C354',
                  padding: 1,
                }}
                onClick={() => appWindow.maximize()}
              >
                <svg
                  xmlns='http://www.w3.org/2000/svg'
                  width='17'
                  height='16'
                  viewBox='0 0 17 16'
                  fill='none'
                >
                  <path
                    d='M4.04504 4.32699C4.04331 3.99321 4.31434 3.72219 4.64812 3.72391L9.99044 3.75145C10.5235 3.7542 10.7885 4.39878 10.4116 4.77571L5.09683 10.0905C4.7199 10.4674 4.07532 10.2024 4.07257 9.66932L4.04504 4.32699Z'
                    fill='#407C33'
                  />
                  <path
                    d='M11.7442 12.0263C12.078 12.028 12.349 11.757 12.3473 11.4232L12.3197 6.08085C12.317 5.5478 11.6724 5.28275 11.2955 5.65968L5.98068 10.9745C5.60376 11.3514 5.86881 11.996 6.40185 11.9987L11.7442 12.0263Z'
                    fill='#407C33'
                  />
                </svg>
              </button>
            </div>
            <SvgTariLaunchpadLogo />
          </div>
          <div style={styles.mainContainer as CSSProperties}>
            <div style={styles.content}>
              <div style={styles.mainMessageContainer}>
                <div style={styles.header}>
                  <SvgTBotBase width={48} height={48} />
                  <h1 style={styles.heading}>Houston, we have a problem!</h1>
                </div>
                <p style={styles.text}>
                  Something went terribly wrong :( Try to restart the
                  application, and if that doesn&apos;t magically fix the
                  problem, feel free to let us know on{' '}
                  <a
                    href={linksConfig.discord}
                    target='_blank'
                    rel='noreferrer'
                    style={styles.linkInText}
                  >
                    Discord
                  </a>
                  .
                </p>
              </div>
              <div>
                <div style={styles.buttons}>
                  <button
                    style={{
                      ...(styles.button as CSSProperties),
                      color: '#20053D',
                      borderWidth: 2,
                      borderStyle: 'solid',
                      borderColor: '#EDECEE',
                      background: '#F6F5F8',
                    }}
                    onClick={() =>
                      this.setState({
                        ...this.state,
                        showDetails: !this.state.showDetails,
                      })
                    }
                  >
                    Show details
                  </button>
                  <button
                    style={styles.button as CSSProperties}
                    onClick={() => appWindow.close()}
                  >
                    Close the app
                  </button>
                </div>
              </div>
              <div
                style={{
                  ...styles.detailsContainer,
                  display: this.state.showDetails ? 'block' : 'none',
                }}
              >
                <p style={styles.detailsText}>{this.state.error}</p>
              </div>
            </div>
          </div>
        </div>
      )
    }

    return this.props.children
  }
}

export default ErrorBoundary
