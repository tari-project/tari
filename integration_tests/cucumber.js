module.exports = {
  default:
    "--tags 'not @long-running and not @wallet-ffi and not @broken' --fail-fast",
  ci: "--tags '@critical and not @long-running and not @broken ' --fail-fast",
  critical: "--format @cucumber/pretty-formatter --tags @critical",
};
