/**
 * Is the given `url` a valid URL?
 * URL has to start with protocol (ie. http/https)
 * @returns {boolean}
 */
export const isUrl = (url: string): boolean => {
  try {
    new URL(url)
    return true
  } catch {
    return false
  }
}
