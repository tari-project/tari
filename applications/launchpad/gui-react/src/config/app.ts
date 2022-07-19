export default {
  // If user dismisses downloading latest Docker image via TBot,
  // after what time we can ask user again? [ms]
  dockerDownloadDismissValidFor: 30 * 1000, // 30 sec
  dockerNewImagesCheckInterval: 600000, // [ms] // @TODO - change to 60000 - using larger until updated field is not fixed
}
