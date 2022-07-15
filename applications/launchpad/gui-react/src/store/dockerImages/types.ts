import { DockerImage, ContainerName, ServiceRecipe } from '../../types/general'

export type Recipes = Record<ContainerName, ServiceRecipe>

export type DockerImagesState = {
  loaded: boolean
  images: DockerImage[]
  recipes: Recipes
  downloadWithTBot: DockerImage[]
  dismissedDownloads: {
    dismissedAt: number // Date as integer [ms]
    containerName: ContainerName
  }[]
}
