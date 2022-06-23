import { DockerImagePullStatus } from '../types/general'

export default {
  settings: {
    title: 'Docker Settings',
    imageStatuses: 'Image Statuses',
    newerVersion: 'A newer version is available',
    pullImage: 'Pull image',
    tagLabel: 'Docker Tag',
    registryLabel: 'Docker Registry',
    status: {
      [DockerImagePullStatus.Ready]: 'ready',
      [DockerImagePullStatus.Pulling]: 'pulling image',
      [DockerImagePullStatus.Waiting]: 'waiting',
    },
  },
}
