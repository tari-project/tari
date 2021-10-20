module.exports = {
  default:
    "--tags 'not @long-running and not @wallet-ffi and not @broken' --fail-fast",
  ci: " ",
  critical: "--format @cucumber/pretty-formatter --tags @critical",
};
