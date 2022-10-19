module.exports = {
  default: "--tags 'not @long-running and not @wallet-ffi and not @broken' ",
  none: " ",
  ci: "--tags '@critical and not @long-running and not @broken '",
  critical: "--format @cucumber/pretty-formatter --tags @critical",
  "non-critical":
    "--tags 'not @critical and not @long-running and not @broken'",
  "long-running": "--tags '@long-running and not @broken'",
};
