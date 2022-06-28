import { createAsyncThunk } from '@reduxjs/toolkit'

import { RootState } from '..'
import {
  selectContainerStatus,
  selectRunningContainers,
} from '../containers/selectors'
import { selectRecipe } from '../dockerImages/selectors'
import { actions as containersActions } from '../containers'
import { Container } from '../containers/types'

export const startNode = createAsyncThunk<void, void, { state: RootState }>(
  'baseNode/startNode',
  async (_, thunkApi) => {
    try {
      const rootState = thunkApi.getState()
      const recipe = selectRecipe(Container.BaseNode)(rootState)

      const recipePromises = [...recipe]
        .reverse()
        .map(part => {
          const status = selectContainerStatus(part)(rootState)
          if (!status.running && !status.pending) {
            return thunkApi
              .dispatch(containersActions.start({ container: part }))
              .unwrap()
          }

          return false
        })
        .filter(Boolean)

      await Promise.all(recipePromises)
    } catch (e) {
      return thunkApi.rejectWithValue(e)
    }
  },
)

export const stopNode = createAsyncThunk<void, void, { state: RootState }>(
  'baseNode/stopNode',
  async (_, thunkApi) => {
    try {
      const rootState = thunkApi.getState()
      const recipe = selectRecipe(Container.BaseNode)(rootState)

      const [head, ...tail] = recipe.map(part =>
        selectContainerStatus(part)(rootState),
      )

      thunkApi.dispatch(containersActions.stop(head.id))

      const runningContainers = selectRunningContainers(rootState)
      const containersOutsideRecipe = runningContainers.filter(
        rc => !recipe.includes(rc),
      )
      const containersRequiredByOtherServices = containersOutsideRecipe.reduce(
        (accu, current) => {
          const currentRecipe = selectRecipe(current)(rootState)
          currentRecipe.forEach(cr => accu.add(cr))

          return accu
        },
        new Set(),
      )

      tail
        .filter(tailPart => !containersRequiredByOtherServices.has(tailPart))
        .forEach(tailPartToStop =>
          thunkApi.dispatch(containersActions.stop(tailPartToStop.id)),
        )
    } catch (e) {
      return thunkApi.rejectWithValue(e)
    }
  },
)
