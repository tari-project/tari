let start: number | null = null
let opacity = 1

const fadeOut = (timestamp: number, splashscreenEl: HTMLElement) => {
  if (!start) {
    start = timestamp
  }
  const progress = timestamp - start
  opacity = 1 - progress / 500
  splashscreenEl.style.opacity = opacity.toString()
  if (opacity > 0) {
    requestAnimationFrame(timestamp => fadeOut(timestamp, splashscreenEl))
  } else {
    splashscreenEl.style.display = 'none'
  }
}

/**
 * Hide splashcreen.
 * The animation takes about 2 seconds.
 *
 * @example
 * hideSplashscreen()
 */
export const hideSplashscreen = () => {
  const splashscreenEl = document.getElementById('splashscreen')

  setTimeout(() => {
    if (splashscreenEl) {
      requestAnimationFrame(timestamp => fadeOut(timestamp, splashscreenEl))
    }
  }, 1000)
}
