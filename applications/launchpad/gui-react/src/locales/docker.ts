import { DockerImagePullStatus } from '../types/general'

export default {
  pullImage: 'Pull image',
  status: {
    [DockerImagePullStatus.Ready]: 'ready',
    [DockerImagePullStatus.Pulling]: 'pulling image',
    [DockerImagePullStatus.Waiting]: 'waiting',
  },
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
}
