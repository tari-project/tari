import { DockerImage, ContainerName, ServiceRecipe } from '../../types/general'

export type Recipes = Record<ContainerName, ServiceRecipe>

export type DockerImagesState = {
  loaded: boolean
  images: DockerImage[]
  recipes: Recipes
}
