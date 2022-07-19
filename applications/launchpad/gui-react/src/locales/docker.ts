export default {
  pullImage: 'Pull image',
  pullNewerImage: 'Pull newer image',
  newerVersion: 'A newer version is available',
  imageUpToDate: 'Image is up to date for',
  settings: {
    title: 'Docker Settings',
    tagLabel: 'Docker Tag',
    registryLabel: 'Docker Registry',
    imageStatuses: 'Image Statuses',
  },
  header: {
    image: 'Image',
    status: 'Status',
  },
  tBot: {
    newVersionAvailable: {
      part1: 'There is a newer version of wallet image ready to pull.',
      part2:
        'Docker images within a running container do not update automatically. Once you have used an image to create a container, it continues running that version, even after new releases come out.',
    },
    downloadStepMessage:
      'It is recommended to run containers from the latest image unless you have a specific reason to use an older release.',
    downloadSuccess: {
      part1: 'You’re good to go!',
      part2: 'Newer version of wallet image has been pulled successfully.',
    },
    downloadError: {
      part1: 'Something went wrong!',
      part2: 'Go to Settings > Docker and try to download the image again.',
    },
  },
}
