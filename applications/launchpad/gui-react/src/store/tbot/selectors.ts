import { RootState } from '..'

export const selectTBotQueue = (state: RootState) => state.tbot.messageQueue
