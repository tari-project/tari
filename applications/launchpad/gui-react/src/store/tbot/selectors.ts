import { RootState } from '..'

export const selectTBotQueue = (state: RootState) => state.tbot.messageQueue
export const selectTBotOpen = (state: RootState) => state.tbot.open
